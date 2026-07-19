use axum::{
    Json,
    body::Bytes,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use chrono::Utc;
use serde::Deserialize;
use shared::lif::LifSummary;
use uuid::Uuid;

use crate::{
    api::{WebApp, error::AppError},
    configuration::configuration::{Configuration, ConfigurationFields},
    runtime::manager::SystemInfo,
};

// get systems
pub async fn get_systems(State(state): State<WebApp>) -> Json<Vec<SystemInfo>> {
    Json(
        state
            .runtimes_manager
            .system_configs()
            .await
            .unwrap_or(Vec::new()),
    )
}

//create system
pub async fn create_system(
    State(state): State<WebApp>,
    Json(body): Json<ConfigurationFields>,
) -> Result<Json<SystemInfo>, AppError> {
    let config = Configuration::new(Uuid::new_v4().to_string(), Utc::now().to_rfc3339(), body);
    let state_info = state.runtimes_manager.add(config).await?;
    Ok(Json(state_info))
}

//update system
pub async fn update_system(
    State(state): State<WebApp>,
    Path(id): Path<String>,
    Json(body): Json<ConfigurationFields>,
) -> Result<Json<SystemInfo>, AppError> {
    let state_info = state.runtimes_manager.update(id, body).await?;
    Ok(Json(state_info))
}

//delete system
pub async fn delete_system(
    State(state): State<WebApp>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    state.runtimes_manager.remove(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

//start system
pub async fn start_system(
    State(state): State<WebApp>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    state.runtimes_manager.start(id).await?;
    Ok(StatusCode::OK)
}

//stop system
pub async fn stop_system(
    State(state): State<WebApp>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    state.runtimes_manager.stop(id).await?;
    Ok(StatusCode::OK)
}

pub async fn upload_lif(
    State(state): State<WebApp>,
    Path(id): Path<String>,
    body: Bytes,
) -> Result<Json<LifSummary>, AppError> {
    if body.is_empty() {
        return Err(AppError::bad_request(anyhow::anyhow!("empty request body")));
    }

    let known = state.runtimes_manager.exists(&id).await;
    if !known {
        return Err(AppError::not_found(anyhow::anyhow!(
            "system '{id}' does not exist"
        )));
    }

    // A parse or validation failure is the client's problem, not ours.
    let summary = state
        .runtimes_manager
        .set_lif(id, body)
        .await
        .map_err(AppError::bad_request)?;

    Ok(Json(summary))
}

/// Fetch a system's stored layout.
///
/// The document is stored gzipped and served that way — the backend never
/// decompresses it, and clients decode it transparently.
pub async fn get_lif(
    State(state): State<WebApp>,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let Some(gzip) = state.runtimes_manager.get_lif_gzip(id.clone()).await? else {
        return Err(AppError::not_found(anyhow::anyhow!(
            "system '{id}' has no layout"
        )));
    };

    Ok((
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CONTENT_ENCODING, "gzip"),
        ],
        gzip,
    )
        .into_response())
}

#[derive(Deserialize)]
pub struct MapQuery {
    /// Which layout to draw. Absent takes the first — a document usually has
    /// only one, and multi-layout files list the rest in `availableLayouts`.
    pub layout: Option<String>,
}

/// Fetch a system's layout as drawable geometry.
///
/// The counterpart to [`get_lif`], for clients that want to render rather than
/// round-trip the document: nodes as points, edges tessellated into polylines,
/// and the bounding box to fit them to a viewport. Small enough to fetch on
/// view — a layout whose source is tens of megabytes projects to hundreds of
/// kilobytes, since the per-vehicle-type properties and actions drop out.
pub async fn get_map(
    State(state): State<WebApp>,
    Path(id): Path<String>,
    Query(query): Query<MapQuery>,
) -> Result<Response, AppError> {
    if !state.runtimes_manager.exists(&id).await {
        return Err(AppError::not_found(anyhow::anyhow!(
            "system '{id}' does not exist"
        )));
    }

    let Some(map) = state.runtimes_manager.get_map(id.clone(), query.layout).await? else {
        return Err(AppError::not_found(anyhow::anyhow!(
            "system '{id}' has no layout"
        )));
    };

    // Serialized by hand rather than returned as `Json<Arc<MapView>>`, which
    // would need serde's `rc` feature — a workspace-wide flag to enable for one
    // response. The `Arc` is what lets the cached view be shared without
    // cloning the geometry, so it is worth keeping.
    let body = serde_json::to_vec(&*map)?;
    Ok(([(header::CONTENT_TYPE, "application/json")], body).into_response())
}

/// Remove a system's stored layout.
pub async fn delete_lif(
    State(state): State<WebApp>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    state.runtimes_manager.delete_lif(id).await?;
    Ok(StatusCode::NO_CONTENT)
}
