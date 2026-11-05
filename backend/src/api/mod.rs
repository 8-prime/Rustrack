use std::{path::Path, sync::Arc};

use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{any, get, post},
};

use crate::{api::error::AppError, runtime::manager::RuntimesManager};

/// Upload ceiling for a LIF layout. Real layouts run to tens of megabytes;
/// this leaves generous headroom while still bounding the request.
const MAX_LIF_UPLOAD_BYTES: usize = 128 * 1024 * 1024;

pub mod error;
pub mod health;
pub mod systems;
pub mod web;
pub mod ws;

#[derive(Clone)]
pub struct WebApp {
    pub runtimes_manager: Arc<RuntimesManager>,
}

pub fn new(data_dir: &Path) -> anyhow::Result<Router> {
    let runtimes_manager = Arc::new(RuntimesManager::new(data_dir)?);

    let web_app_state = WebApp { runtimes_manager };

    let router = Router::new();
    let app = router
        .route("/health", get(health::handler))
        .route(
            "/api/systems",
            get(systems::get_systems).post(systems::create_system),
        )
        .route(
            "/api/systems/{id}",
            get(systems::get_system)
                .put(systems::update_system)
                .delete(systems::delete_system),
        )
        .route("/api/systems/{id}/start", post(systems::start_system))
        .route("/api/systems/{id}/stop", post(systems::stop_system))
        .route(
            "/api/systems/{id}/lif",
            get(systems::get_lif)
                .post(systems::upload_lif)
                .delete(systems::delete_lif)
                .layer(DefaultBodyLimit::max(MAX_LIF_UPLOAD_BYTES)),
        )
        // The drawable projection of the same layout. Read-only, so no body
        // limit applies.
        .route("/api/systems/{id}/map", get(systems::get_map))
        .route("/api/systems/{id}/ws", get(ws::handler))
        // reject any unknown paths under /api/
        .route(
            "/api/{*rest}",
            any(async || AppError::not_found(anyhow::anyhow!("unknown endpoint"))),
        )
        // Everything else is the embedded frontend.
        .fallback(get(web::handler))
        .with_state(web_app_state);

    Ok(app)
}
