//! Web Search Load Balance - MCP Server
//!
//! An MCP server that provides web_search and web_fetch tools
//! with load balancing across multiple providers and API keys.

use std::path::PathBuf;

use anyhow::Context;
use one_search::config::Config;
use one_search::tools::WebSearchMcpServer;
use one_search::WebSearchError;
use rmcp::transport::async_rw::AsyncRwTransport;
use tracing_subscriber::EnvFilter;

fn default_config_path() -> PathBuf {
    PathBuf::from("config.yaml")
}

fn fallback_config_path() -> PathBuf {
    if let Some(home) = std::env::var_os("HOME") {
        PathBuf::from(home).join(".config").join("websearch.yaml")
    } else {
        PathBuf::from("config.yaml")
    }
}

fn load_config() -> Result<Config, WebSearchError> {
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
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .init();

    tracing::info!("Starting web search load balance MCP server");

    let config = load_config().context("Failed to load configuration")?;

    let provider_count = config.enabled_providers().len();
    tracing::info!("Loaded {} provider(s)", provider_count);

    let server = WebSearchMcpServer::new(&config).context("Failed to create MCP server")?;

    // Check HTTP config
    if let Some(http) = &config.server.http {
        if http.enabled {
            run_http_server(server, http.clone()).await?;
            return Ok(());
        }
    }

    // Default: stdio mode
    tracing::info!("MCP server initialized, waiting for connections via stdio...");

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let transport = AsyncRwTransport::new_server(stdin, stdout);
    rmcp::service::serve_server(server, transport)
        .await
        .context("Failed to serve MCP")?;

    Ok(())
}

async fn run_http_server(
    server: WebSearchMcpServer,
    http_config: one_search::config::HttpConfig,
) -> anyhow::Result<()> {
    let host = http_config.host;
    let port = http_config.port;
    let mcp_path = http_config.mcp_path.unwrap_or_else(|| "/mcp".to_string());
    let api_key = http_config.api_key;

    tracing::info!("HTTP server listening on http://{}:{}", host, port);
    tracing::info!("MCP endpoint: http://{}:{}{}", host, port, mcp_path);

    if api_key.is_some() {
        tracing::info!("API key authentication enabled");
    }

    let ct = tokio_util::sync::CancellationToken::new();
    let app = one_search::build_router(
        server,
        &mcp_path,
        api_key,
        http_config.allowed_hosts,
        ct.clone(),
    );

    let addr: std::net::SocketAddr = format!("{}:{}", host, port)
        .parse()
        .context("Invalid address")?;

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .context(format!("Failed to bind to {}", addr))?;

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            tokio::signal::ctrl_c().await.unwrap();
            ct.cancel();
        })
        .await?;

    Ok(())
}
