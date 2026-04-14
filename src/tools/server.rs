//! MCP server implementation for web search tools.

use crate::config::Config;
use crate::error::WebSearchError;
use crate::load_balancer::ProviderLoadBalancer;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::ServerInfo;
use rmcp::schemars;
use rmcp::{tool, tool_handler, tool_router, ErrorData as McpError, ServerHandler};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Input schema for web_search tool.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SearchParams {
    /// The search query string. Use 3-5 keywords for best results.
    pub query: String,

    /// Maximum number of results to return (default: 10).
    #[serde(default = "default_max_results")]
    pub max_results: u32,
}

fn default_max_results() -> u32 {
    10
}

/// Input schema for web_fetch tool.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct FetchParams {
    /// The URL to fetch content from.
    pub url: String,
}

/// MCP server for web search load balancing.
pub struct WebSearchMcpServer {
    tool_router: ToolRouter<Self>,
    load_balancer: ProviderLoadBalancer,
}

impl WebSearchMcpServer {
    /// Create a new MCP server from configuration.
    pub fn new(config: &Config) -> Result<Self, WebSearchError> {
        let load_balancer = ProviderLoadBalancer::from_config(config)?;
        Ok(Self {
            tool_router: Self::tool_router(),
            load_balancer,
        })
    }
}

#[tool_router]
impl WebSearchMcpServer {
    /// Web search tool - searches using configured providers with load balancing.
    #[tool(
        description = "Search the web for information. Uses multiple providers (Tavily, MiniMax, ZhiPu) \
            with load balancing across API keys for reliability and rate limit handling."
    )]
    fn web_search(
        &self,
        Parameters(params): Parameters<SearchParams>,
    ) -> Result<String, McpError> {
        // Note: Using sync function for rmcp compatibility
        // For async, would need different approach
        let runtime = tokio::runtime::Handle::current();
        runtime.block_on(async {
            match self.load_balancer.search(&params.query, params.max_results).await {
                Ok(response) => {
                    Ok(serde_json::to_string_pretty(&response)
                        .unwrap_or_else(|_| format!("{:?}", response)))
                }
                Err(e) => {
                    Ok(format!("Search failed: {}", e))
                }
            }
        })
    }

    /// Web fetch tool - fetches and extracts content from a URL.
    #[tool(
        description = "Fetch and extract content from a URL. Returns markdown-formatted content. \
            Uses Tavily or ZhiPu provider (MiniMax does not support fetch)."
    )]
    fn web_fetch(
        &self,
        Parameters(params): Parameters<FetchParams>,
    ) -> Result<String, McpError> {
        let runtime = tokio::runtime::Handle::current();
        runtime.block_on(async {
            match self.load_balancer.fetch(&params.url).await {
                Ok(response) => {
                    Ok(serde_json::to_string_pretty(&response)
                        .unwrap_or_else(|_| format!("{:?}", response)))
                }
                Err(e) => {
                    Ok(format!("Fetch failed: {}", e))
                }
            }
        })
    }
}

impl Default for WebSearchMcpServer {
    fn default() -> Self {
        Self {
            tool_router: Self::tool_router(),
            load_balancer: ProviderLoadBalancer::from_config(&Config::default())
                .expect("default config should work"),
        }
    }
}

#[tool_handler]
impl ServerHandler for WebSearchMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::default()
    }
}