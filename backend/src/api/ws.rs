use axum::{
    extract::{
        Path, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::StatusCode,
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use shared::protocol::protocol::{AgvRecord, encode_frame};
use tokio::sync::broadcast::{self, error::RecvError};

use crate::{api::WebApp, runtime::publisher::StateSnapshot};

pub async fn handler(
    ws: WebSocketUpgrade,
    Path(id): Path<String>,
    State(app): State<WebApp>,
) -> Response {
    let rx = {
        let runtimes = app.runtimes_manager.runtimes.read().await;
        match runtimes.get(&id) {
            Some(runtime) => runtime.publisher.subscribe(),
            None => return (StatusCode::NOT_FOUND, "unknown system").into_response(),
        }
    };

    ws.on_upgrade(move |socket| stream_states(socket, rx))
}

async fn stream_states(mut socket: WebSocket, mut rx: broadcast::Receiver<StateSnapshot>) {
    loop {
        tokio::select! {
            recv = rx.recv() => match recv {
                Ok(snapshot) => {
                    let frame = encode_snapshot(&snapshot);
                    if socket.send(Message::Binary(frame)).await.is_err() {
                        break; // client's receive half is gone
                    }
                }
                Err(RecvError::Lagged(_)) => continue,
                Err(RecvError::Closed) => break,
            },

            client_msg = socket.recv() => match client_msg {
                Some(Ok(Message::Close(_))) | None => break,
                Some(Err(_)) => break,
                Some(Ok(_)) => {}
            },
        }
    }
}

/// Flatten a state snapshot into the compact binary wire frame the renderer reads.
fn encode_snapshot(snapshot: &StateSnapshot) -> Bytes {
    let records: Vec<AgvRecord> = snapshot
        .iter()
        .filter_map(|(serial, robot)| {
            let (x, y, theta) = match &robot.interpolated_state {
                Some(i) => (i.x, i.y, i.theta),
                None => {
                    let p = robot.vda_state.position.as_ref()?;
                    (p.x, p.y, p.theta)
                }
            };
            Some(AgvRecord {
                serial: serial.clone(),
                x,
                y,
                theta,
            })
        })
        .collect();

    encode_frame(&records)
}
