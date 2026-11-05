use axum::{
    extract::{
        Path, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::StatusCode,
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use shared::protocol::protocol::{AgvRecord, encode_frame};
use std::sync::Arc;
use tokio::sync::broadcast::{self, error::RecvError};

use crate::{
    api::WebApp,
    runtime::{
        publisher::StateSnapshot,
        route::{PlannedPath, Route},
        state::StateManager,
    },
};

/// Client request, sent as a text frame: which robot's route to stream. A `null`
/// serial stops the stream.
#[derive(Deserialize)]
struct SelectRoute {
    serial: Option<String>,
}

/// Server message, sent as a text frame whenever the selected robot's route
/// changes. Pose frames stay binary, so a client that never selects a robot
/// sees exactly what it always did.
#[derive(Serialize)]
struct RouteMessage<'a> {
    serial: &'a str,
    #[serde(flatten)]
    route: Route,
}

pub async fn handler(
    ws: WebSocketUpgrade,
    Path(id): Path<String>,
    State(app): State<WebApp>,
) -> Response {
    let (rx, state_manager) = {
        let runtimes = app.runtimes_manager.runtimes.read().await;
        match runtimes.get(&id) {
            Some(runtime) => (runtime.publisher.subscribe(), runtime.state_manager.clone()),
            None => {
                tracing::warn!("websocket connect for unknown system '{id}'");
                return (StatusCode::NOT_FOUND, "unknown system").into_response();
            }
        }
    };

    tracing::debug!("websocket client subscribed to system '{id}'");
    ws.on_upgrade(move |socket| stream_states(socket, rx, state_manager, id))
}

async fn stream_states(
    mut socket: WebSocket,
    mut rx: broadcast::Receiver<StateSnapshot>,
    state_manager: Arc<StateManager>,
    id: String,
) {
    let mut selected: Option<String> = None;
    // The path this client last got a route for. Tessellating is left to here,
    // rather than done per MQTT message, so only robots someone is looking at
    // pay for it, and only when their path actually changes.
    let mut sent: Option<Arc<PlannedPath>> = None;

    loop {
        tokio::select! {
            recv = rx.recv() => match recv {
                Ok(snapshot) => {
                    let frame = encode_snapshot(&snapshot);
                    tracing::trace!(
                        "sending frame ({} bytes) to '{id}' websocket client",
                        frame.len()
                    );
                    if socket.send(Message::Binary(frame)).await.is_err() {
                        tracing::debug!("websocket client for '{id}' disconnected (send failed)");
                        break; // client's receive half is gone
                    }
                    if let Some(message) = route_update(&state_manager, selected.as_deref(), &mut sent)
                        && socket.send(message).await.is_err()
                    {
                        tracing::debug!("websocket client for '{id}' disconnected (send failed)");
                        break;
                    }
                }
                Err(RecvError::Lagged(skipped)) => {
                    tracing::debug!("websocket client for '{id}' lagged, skipped {skipped} snapshot(s)");
                    continue;
                }
                Err(RecvError::Closed) => {
                    tracing::debug!("broadcast channel for '{id}' closed, ending websocket stream");
                    break;
                }
            },

            client_msg = socket.recv() => match client_msg {
                Some(Ok(Message::Close(_))) | None => {
                    tracing::debug!("websocket client for '{id}' closed the connection");
                    break;
                }
                Some(Err(_)) => break,
                Some(Ok(Message::Text(text))) => match serde_json::from_str::<SelectRoute>(&text) {
                    Ok(request) => {
                        selected = request.serial;
                        sent = None;
                    }
                    Err(e) => tracing::debug!("ignoring websocket message from '{id}' client: {e}"),
                },
                Some(Ok(_)) => {}
            },
        }
    }
}

/// The route message for the selected robot, if its path changed since `sent`.
fn route_update(
    state_manager: &StateManager,
    selected: Option<&str>,
    sent: &mut Option<Arc<PlannedPath>>,
) -> Option<Message> {
    let serial = selected?;
    let path = match state_manager.path(serial) {
        Ok(path) => path?,
        Err(e) => {
            tracing::error!("skipping route update: {e}");
            return None;
        }
    };
    if sent.as_ref().is_some_and(|s| Arc::ptr_eq(s, &path)) {
        return None;
    }

    let message = RouteMessage {
        serial,
        route: Route::from(path.as_ref()),
    };
    let json = match serde_json::to_string(&message) {
        Ok(json) => json,
        Err(e) => {
            tracing::error!("failed to encode route for '{serial}': {e}");
            return None;
        }
    };
    *sent = Some(path);
    Some(Message::Text(json.into()))
}

/// Flatten a state snapshot into the compact binary wire frame the renderer reads.
fn encode_snapshot(snapshot: &StateSnapshot) -> Bytes {
    let records: Vec<AgvRecord> = snapshot
        .iter()
        .map(|s| AgvRecord {
            serial: s.serial.clone(),
            x: s.x,
            y: s.y,
            theta: s.theta,
        })
        .collect();

    tracing::trace!("encoded {} robot(s) into frame", records.len());

    encode_frame(&records)
}
