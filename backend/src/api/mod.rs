use std::sync::Arc;

use axum::{
    Router,
    routing::{get, post, put},
};

use crate::runtime::manager::RuntimesManager;

pub mod error;
pub mod health;
pub mod systems;
pub mod ws;

#[derive(Clone)]
pub struct WebApp {
    pub runtimes_manager: Arc<RuntimesManager>,
}

pub fn new() -> anyhow::Result<Router> {
    let runtimes_manager = Arc::new(RuntimesManager::new()?);

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
            put(systems::update_system).delete(systems::delete_system),
        )
        .route("/api/systems/{id}/start", post(systems::start_system))
        .route("/api/systems/{id}/stop", post(systems::stop_system))
        .route("/api/systems/{id}/ws", get(ws::handler))
        .with_state(web_app_state);

    Ok(app)
}
