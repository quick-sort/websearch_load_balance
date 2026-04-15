// Library root - re-exports for convenience

pub mod config;
pub mod error;
pub mod load_balancer;
pub mod providers;
pub mod tools;

pub use config::Config;
pub use error::WebSearchError;

use axum::{http::StatusCode, response::IntoResponse, routing::get, Router};
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager,
    tower::{StreamableHttpServerConfig, StreamableHttpService},
};
use tools::WebSearchMcpServer;
use tower_http::cors::{Any, CorsLayer};

async fn health_handler() -> impl IntoResponse {
    (StatusCode::OK, "{\"status\":\"ok\"}")
}

/// Build the axum Router for the HTTP MCP server.
pub fn build_router(
    server: WebSearchMcpServer,
    mcp_path: &str,
    api_key: Option<String>,
    ct: tokio_util::sync::CancellationToken,
) -> Router {
    let mcp_service = StreamableHttpService::new(
        move || Ok(server.clone()),
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default().with_cancellation_token(ct.child_token()),
    );

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let mut app = Router::new().route("/health", get(health_handler));

    if let Some(key) = api_key {
        let key = std::sync::Arc::new(key);
        let auth_middleware = axum::middleware::from_fn(
            move |req: axum::extract::Request, next: axum::middleware::Next| {
                let key = key.clone();
                async move {
                    let provided = req
                        .headers()
                        .get("authorization")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.strip_prefix("Bearer "))
                        .map(|s| s.to_string());
                    if provided.as_deref() != Some(key.as_str()) {
                        return Err((StatusCode::UNAUTHORIZED, "Invalid or missing API key"));
                    }
                    Ok(next.run(req).await)
                }
            },
        );
        // Auth layer only on the MCP path, not on /health
        let mcp_router = Router::new()
            .nest_service(mcp_path, mcp_service)
            .layer(auth_middleware);
        app = app.merge(mcp_router);
    } else {
        app = app.nest_service(mcp_path, mcp_service);
    }

    app.layer(cors)
}
