use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use chrono::Utc;
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    api::{WebApp, error::AppError},
    configuration::configuration::Configuration,
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

#[derive(Clone, Deserialize)]
pub struct CreateSystem {
    pub name: String,
    pub mqtt_url: String,
    pub mqtt_port: u16,
    pub mqtt_username: Option<String>,
    pub mqtt_password: Option<String>,
    pub tls_skip_verify: bool,
    pub vda5050_topic_prefix: String,
}

//create system
pub async fn create_system(
    State(state): State<WebApp>,
    Json(body): Json<CreateSystem>,
) -> Result<Json<SystemInfo>, AppError> {
    let id = Uuid::new_v4().to_string();
    let state_info = state
        .runtimes_manager
        .add(Configuration {
            created_at: Utc::now().to_rfc3339(),
            id: id,
            mqtt_password: body.mqtt_password,
            mqtt_port: body.mqtt_port,
            mqtt_url: body.mqtt_url,
            mqtt_username: body.mqtt_username,
            name: body.name,
            tls_skip_verify: body.tls_skip_verify,
            vda5050_topic_prefix: body.vda5050_topic_prefix,
        })
        .await?;
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
