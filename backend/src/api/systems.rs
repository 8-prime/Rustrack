use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use chrono::Utc;
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
