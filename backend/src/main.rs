mod api;
mod cli;
mod configuration;
mod interpolation;
mod mqtt;
mod persistence;
pub mod runtime;
mod server;
#[cfg(windows)]
mod service;

use std::path::Path;

use clap::Parser;
use cli::{Cli, ServerArgs};
use tracing_subscriber::EnvFilter;

fn log_filter() -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into())
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    #[cfg(windows)]
    if let Some(cli::Command::Service(command)) = cli.command {
        return service::dispatch(command, cli.server);
    }

    tracing_subscriber::fmt()
        .with_env_filter(log_filter())
        .init();
    run_console(cli.server)
}

#[tokio::main]
async fn run_console(args: ServerArgs) -> anyhow::Result<()> {
    let data_dir = args.data_dir.as_deref().unwrap_or(Path::new("."));
    let server = server::Server::bind(&args.listen, data_dir).await?;
    server
        .serve(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
}
