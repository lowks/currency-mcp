mod client;
mod server;
mod types;

use anyhow::Context;
use rmcp::{ServiceExt, transport::stdio};
use tracing_subscriber::EnvFilter;

use crate::client::FrankfurterClient;
use crate::server::CurrencyServer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_ansi(false)
        .init();

    let client = FrankfurterClient::new().context("failed to build HTTP client")?;
    let server = CurrencyServer::new(client);

    tracing::info!("starting currency MCP server on stdio");
    let service = server.serve(stdio()).await.inspect_err(|error| {
        tracing::error!("failed to start MCP server: {error:#}");
    })?;
    service.waiting().await?;
    Ok(())
}
