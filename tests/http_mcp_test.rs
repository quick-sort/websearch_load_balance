//! Integration tests for the HTTP MCP server.

use one_search::config::{Config, LoadBalanceStrategy, ProviderConfig};
use one_search::error::parse_api_key;
use one_search::tools::WebSearchMcpServer;
use rmcp::model::CallToolRequestParams;
use rmcp::transport::streamable_http_client::{
    StreamableHttpClientTransport, StreamableHttpClientTransportConfig,
};
use rmcp::{ClientHandler, ServiceExt};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Default)]
struct TestClient;

impl ClientHandler for TestClient {
    fn get_info(&self) -> rmcp::model::ClientInfo {
        rmcp::model::ClientInfo::default()
    }
}

fn test_config() -> Config {
    let mut providers = Vec::new();

    let tavily_key = parse_api_key("TAVILY_API_KEYS");
    if !tavily_key.is_empty() {
        providers.push(ProviderConfig {
            name: "tavily".to_string(),
            enabled: true,
            base_url: String::new(),
            api_keys: vec![tavily_key],
            settings: Default::default(),
        });
    }

    let zhipu_key = parse_api_key("GLM_API_KEYS");
    if !zhipu_key.is_empty() {
        providers.push(ProviderConfig {
            name: "zhipu".to_string(),
            enabled: true,
            base_url: String::new(),
            api_keys: vec![zhipu_key],
            settings: Default::default(),
        });
    }

    if providers.is_empty() {
        panic!("No API keys found. Set TAVILY_API_KEYS or GLM_API_KEYS.");
    }

    Config {
        server: Default::default(),
        key_strategy: Default::default(),
        provider_strategy: LoadBalanceStrategy {
            r#type: one_search::config::StrategyType::RoundRobin,
            fallback: true,
        },
        providers,
        paper_providers: vec![],
    }
}

async fn start_server(api_key: Option<String>) -> (String, CancellationToken) {
    let server = WebSearchMcpServer::new(&test_config()).expect("create server");
    let ct = CancellationToken::new();
    let app = one_search::build_router(server, "/mcp", api_key, vec![], ct.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());

    let ct2 = ct.clone();
    tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move { ct2.cancelled().await })
            .await
            .unwrap();
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    (base_url, ct)
}

#[tokio::test]
#[ignore]
async fn test_health_endpoint() {
    let (url, ct) = start_server(None).await;

    let resp = reqwest::get(format!("{}/health", url)).await.unwrap();
    assert_eq!(resp.status(), 200);
    assert!(resp.text().await.unwrap().contains("ok"));

    ct.cancel();
}

#[tokio::test]
#[ignore]
async fn test_mcp_list_tools() {
    let (url, ct) = start_server(None).await;

    let client = TestClient
        .serve(StreamableHttpClientTransport::from_uri(format!(
            "{}/mcp",
            url
        )))
        .await
        .unwrap();

    let tools = client.list_all_tools().await.unwrap();
    let names: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();
    assert!(
        names.iter().any(|n| n == "web_search"),
        "missing web_search: {:?}",
        names
    );
    assert!(
        names.iter().any(|n| n == "web_fetch"),
        "missing web_fetch: {:?}",
        names
    );

    client.cancel().await.unwrap();
    ct.cancel();
}

#[tokio::test]
#[ignore]
async fn test_mcp_web_search() {
    let (url, ct) = start_server(None).await;

    let client = TestClient
        .serve(StreamableHttpClientTransport::from_uri(format!(
            "{}/mcp",
            url
        )))
        .await
        .unwrap();

    let result = client
        .call_tool(
            CallToolRequestParams::new("web_search").with_arguments(
                serde_json::json!({"query": "Rust programming language", "max_results": 3})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();

    assert!(!result.content.is_empty(), "empty result");
    let raw = serde_json::to_string(&result.content[0]).unwrap();
    assert!(
        raw.to_lowercase().contains("rust"),
        "should mention Rust: {}",
        raw
    );

    client.cancel().await.unwrap();
    ct.cancel();
}

#[tokio::test]
#[ignore]
async fn test_mcp_web_fetch() {
    let (url, ct) = start_server(None).await;

    let client = TestClient
        .serve(StreamableHttpClientTransport::from_uri(format!(
            "{}/mcp",
            url
        )))
        .await
        .unwrap();

    let result = client
        .call_tool(
            CallToolRequestParams::new("web_fetch").with_arguments(
                serde_json::json!({"url": "https://www.rust-lang.org/"})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();

    assert!(!result.content.is_empty(), "empty result");
    let raw = serde_json::to_string(&result.content[0]).unwrap();
    assert!(raw.to_lowercase().contains("rust"), "should mention Rust");

    client.cancel().await.unwrap();
    ct.cancel();
}

#[tokio::test]
#[ignore]
async fn test_mcp_auth_reject_unauthenticated() {
    let (url, ct) = start_server(Some("test-secret".to_string())).await;

    let resp = reqwest::Client::new()
        .post(format!("{}/mcp", url))
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .body(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"t","version":"0.1"}}}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);

    ct.cancel();
}

#[tokio::test]
#[ignore]
async fn test_mcp_auth_accept_authenticated() {
    let (url, ct) = start_server(Some("test-secret".to_string())).await;

    let client = TestClient
        .serve(StreamableHttpClientTransport::from_config(
            StreamableHttpClientTransportConfig::with_uri(format!("{}/mcp", url))
                .auth_header("test-secret"),
        ))
        .await
        .unwrap();

    let tools = client.list_all_tools().await.unwrap();
    assert!(!tools.is_empty(), "should list tools with valid auth");

    client.cancel().await.unwrap();
    ct.cancel();
}

#[tokio::test]
#[ignore]
async fn test_mcp_call_nonexistent_tool() {
    let (url, ct) = start_server(None).await;

    let client = TestClient
        .serve(StreamableHttpClientTransport::from_uri(format!(
            "{}/mcp",
            url
        )))
        .await
        .unwrap();

    let result = client
        .call_tool(CallToolRequestParams::new("nonexistent_tool"))
        .await;
    assert!(result.is_err(), "calling nonexistent tool should error");

    client.cancel().await.unwrap();
    ct.cancel();
}

#[tokio::test]
#[ignore]
async fn test_mcp_no_auth_config_allows_access() {
    // When no api_key is configured, MCP endpoint should be publicly accessible
    let (url, ct) = start_server(None).await;

    // Raw POST without any auth header should succeed (not 401)
    let resp = reqwest::Client::new()
        .post(format!("{}/mcp", url))
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .body(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"t","version":"0.1"}}}"#)
        .send()
        .await
        .unwrap();
    assert_ne!(
        resp.status(),
        401,
        "should not require auth when api_key is not configured"
    );
    assert!(
        resp.status().is_success(),
        "should succeed without auth, got {}",
        resp.status()
    );

    ct.cancel();
}

#[tokio::test]
#[ignore]
async fn test_mcp_paper_search() {
    let (url, ct) = start_server(None).await;

    let client = TestClient
        .serve(StreamableHttpClientTransport::from_uri(format!(
            "{}/mcp",
            url
        )))
        .await
        .unwrap();

    let result = client
        .call_tool(
            CallToolRequestParams::new("paper_search").with_arguments(
                serde_json::json!({
                    "query": "machine learning",
                    "n": 3,
                    "sources": ["arxiv"]
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        )
        .await
        .unwrap();

    assert!(!result.content.is_empty(), "empty result");
    let raw = serde_json::to_string(&result.content[0]).unwrap();
    assert!(
        raw.contains("papers"),
        "should contain papers field: {}",
        raw
    );

    client.cancel().await.unwrap();
    ct.cancel();
}

#[tokio::test]
#[ignore]
async fn test_mcp_paper_fetch() {
    let (url, ct) = start_server(None).await;

    let client = TestClient
        .serve(StreamableHttpClientTransport::from_uri(format!(
            "{}/mcp",
            url
        )))
        .await
        .unwrap();

    let result = client
        .call_tool(
            CallToolRequestParams::new("paper_fetch").with_arguments(
                serde_json::json!({
                    "arxiv_id": "2106.09685",
                    "sources": ["arxiv"]
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        )
        .await
        .unwrap();

    assert!(!result.content.is_empty(), "empty result");
    let raw = serde_json::to_string(&result.content[0]).unwrap();
    // Should contain paper content about LoRA
    assert!(
        raw.to_lowercase().contains("lora") || raw.to_lowercase().contains("low-rank"),
        "should mention LoRA: {}",
        raw
    );

    client.cancel().await.unwrap();
    ct.cancel();
}

#[tokio::test]
#[ignore]
async fn test_mcp_list_tools_includes_paper_tools() {
    let (url, ct) = start_server(None).await;

    let client = TestClient
        .serve(StreamableHttpClientTransport::from_uri(format!(
            "{}/mcp",
            url
        )))
        .await
        .unwrap();

    let tools = client.list_all_tools().await.unwrap();
    let names: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();
    assert!(
        names.iter().any(|n| n == "paper_search"),
        "missing paper_search: {:?}",
        names
    );
    assert!(
        names.iter().any(|n| n == "paper_fetch"),
        "missing paper_fetch: {:?}",
        names
    );

    client.cancel().await.unwrap();
    ct.cancel();
}
