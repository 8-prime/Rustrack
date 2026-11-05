use std::{future::Future, path::Path};

use axum::Router;
use tokio::net::TcpListener;

use crate::api;

/// A server that has claimed its port and opened its database, so everything
/// that can plausibly fail at startup has already happened.
pub struct Server {
    listener: TcpListener,
    router: Router,
}

impl Server {
    pub async fn bind(listen: &str, data_dir: &Path) -> anyhow::Result<Self> {
        let router = api::new(data_dir)?;
        let listener = TcpListener::bind(listen).await?;
        tracing::info!("backend listening on {}", listener.local_addr()?);
        Ok(Self { listener, router })
    }

    /// Serve until `shutdown` resolves, then let in-flight requests finish.
    pub async fn serve(
        self,
        shutdown: impl Future<Output = ()> + Send + 'static,
    ) -> anyhow::Result<()> {
        axum::serve(self.listener, self.router)
            .with_graceful_shutdown(shutdown)
            .await?;
        tracing::info!("backend stopped");
        Ok(())
    }
}
