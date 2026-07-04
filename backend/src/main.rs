mod api;
mod configuration;
mod interpolation;
mod mqtt;
mod persistence;
pub mod runtime;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let router = api::new();

    let addr = "0.0.0.0:3000";
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, router).await?;

    Ok(())
}
