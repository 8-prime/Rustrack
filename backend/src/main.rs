mod api;
mod configuration;
mod interpolation;
mod mqtt;
mod persistence;
pub mod runtime;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let router = api::new();

    let addr = "0.0.0.0:3000";
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("backend listening on {addr}");
    axum::serve(listener, router).await?;

    Ok(())
}
