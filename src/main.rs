//! Web Search Load Balance - MCP Server
//!
//! An MCP server that provides web_search and web_fetch tools
//! with load balancing across multiple providers and API keys.
//! Supports stdio transport (HTTP can be added later).

use std::path::PathBuf;

use anyhow::Context;
use rmcp::transport::async_rw::AsyncRwTransport;
use tracing_subscriber::EnvFilter;
use websearch_load_balance::config::Config;
use websearch_load_balance::tools::WebSearchMcpServer;
use websearch_load_balance::WebSearchError;

/// Default config file path: ./config.yaml
fn default_config_path() -> PathBuf {
    PathBuf::from("config.yaml")
}

/// Fallback config file path: ~/.config/websearch.yaml
fn fallback_config_path() -> PathBuf {
    if let Some(home) = std::env::var_os("HOME") {
        PathBuf::from(home).join(".config").join("websearch.yaml")
    } else {
        PathBuf::from("config.yaml")
    }
}

/// Try to load config from candidate paths.
fn load_config() -> Result<Config, WebSearchError> {
    // Check command line args first
    let args: Vec<String> = std::env::args().collect();
    for (i, arg) in args.iter().enumerate() {
        if arg == "--config" {
            if let Some(path) = args.get(i + 1) {
                return Config::load(path).map_err(|e| WebSearchError::ConfigError(e.to_string()));
            }
        }
        if arg.starts_with("--config=") {
            let path = arg.strip_prefix("--config=").unwrap();
            return Config::load(path).map_err(|e| WebSearchError::ConfigError(e.to_string()));
        }
    }

    // Try default path, then fallback
    let default_path = default_config_path();
    if default_path.exists() {
        tracing::info!("Loading config from: {:?}", default_path);
        return Config::load(&default_path).map_err(|e| WebSearchError::ConfigError(e.to_string()));
    }

    let fallback = fallback_config_path();
    if fallback.exists() {
        tracing::info!("Loading config from: {:?}", fallback);
        return Config::load(&fallback).map_err(|e| WebSearchError::ConfigError(e.to_string()));
    }

    Err(WebSearchError::ConfigError(format!(
        "No config file found. Tried: {:?}, {:?}",
        default_path, fallback
    )))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .init();

    tracing::info!("Starting web search load balance MCP server");

    // Load configuration
    let config = load_config().context("Failed to load configuration")?;

    let provider_count = config.enabled_providers().len();
    tracing::info!("Loaded {} provider(s)", provider_count);

    tracing::info!("MCP server initialized, waiting for connections via stdio...");

    // Create MCP server
    let server = WebSearchMcpServer::new(&config)
        .context("Failed to create MCP server")?;

    // Serve via stdio transport
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let transport = AsyncRwTransport::new_server(stdin, stdout);
    rmcp::service::serve_server(server, transport)
        .await
        .context("Failed to serve MCP")?;

    Ok(())
}