//! MCP server implementation for web search and paper search tools.

use crate::config::Config;
use crate::error::WebSearchError;
use crate::load_balancer::ProviderLoadBalancer;
use crate::paper_load_balancer::PaperLoadBalancer;
use crate::paper_providers::trait_def::PaperId;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{ServerCapabilities, ServerInfo};
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

/// Input schema for paper_search tool.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct PaperSearchParams {
    /// The search query string for academic papers.
    pub query: String,

    /// Maximum number of results to return (default: 10).
    #[serde(default = "default_max_results")]
    pub n: u32,

    /// Academic sources to search. Available: google_scholar, pubmed, arxiv, biorxiv, medrxiv, semantic.
    /// Default: ["google_scholar"].
    #[serde(default = "default_paper_sources")]
    pub sources: Vec<String>,
}

/// Input schema for paper_fetch tool.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct PaperFetchParams {
    /// Paper identifier with one or more ID types. At least one field must be provided.
    /// Fields: doi, pmid, pmcid, arxiv_id, semantic_id, url.
    #[serde(flatten)]
    pub id: PaperId,

    /// Sources to try for fetching. Available: arxiv, semantic, sci_hub, pmc, pubmed.
    /// Default: ["arxiv", "semantic", "sci_hub"].
    #[serde(default = "default_fetch_sources")]
    pub sources: Vec<String>,
}

fn default_paper_sources() -> Vec<String> {
    vec!["google_scholar".into()]
}

fn default_fetch_sources() -> Vec<String> {
    vec!["arxiv".into(), "semantic".into(), "sci_hub".into()]
}

/// MCP server for web search and paper search load balancing.
#[derive(Clone)]
pub struct WebSearchMcpServer {
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
    load_balancer: ProviderLoadBalancer,
    paper_load_balancer: PaperLoadBalancer,
}

impl WebSearchMcpServer {
    /// Create a new MCP server from configuration.
    pub fn new(config: &Config) -> Result<Self, WebSearchError> {
        let load_balancer = ProviderLoadBalancer::from_config(config)?;
        let paper_load_balancer = PaperLoadBalancer::from_config(config);
        Ok(Self {
            tool_router: Self::tool_router(),
            load_balancer,
            paper_load_balancer,
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
    async fn web_search(
        &self,
        Parameters(params): Parameters<SearchParams>,
    ) -> Result<String, McpError> {
        tracing::info!(
            "web_search called: query={:?}, max_results={}",
            params.query,
            params.max_results
        );
        match self
            .load_balancer
            .search(&params.query, params.max_results)
            .await
        {
            Ok(response) => Ok(serde_json::to_string_pretty(&response)
                .unwrap_or_else(|_| format!("{:?}", response))),
            Err(e) => Ok(format!("Search failed: {}", e)),
        }
    }

    /// Web fetch tool - fetches and extracts content from a URL.
    #[tool(
        description = "Fetch and extract content from a URL. Returns markdown-formatted content. \
            Uses Tavily or ZhiPu provider (MiniMax does not support fetch)."
    )]
    async fn web_fetch(
        &self,
        Parameters(params): Parameters<FetchParams>,
    ) -> Result<String, McpError> {
        tracing::info!("web_fetch called: url={:?}", params.url);
        match self.load_balancer.fetch(&params.url).await {
            Ok(response) => Ok(serde_json::to_string_pretty(&response)
                .unwrap_or_else(|_| format!("{:?}", response))),
            Err(e) => Ok(format!("Fetch failed: {}", e)),
        }
    }

    /// Paper search tool - searches academic papers across multiple sources.
    #[tool(
        description = "Search academic papers across multiple sources (Google Scholar, PubMed, arXiv, \
            bioRxiv, medRxiv, Semantic Scholar). Specify sources to search via the 'sources' parameter. \
            Results are merged from all requested sources."
    )]
    async fn paper_search(
        &self,
        Parameters(params): Parameters<PaperSearchParams>,
    ) -> Result<String, McpError> {
        tracing::info!(
            "paper_search called: query={:?}, n={}, sources={:?}",
            params.query,
            params.n,
            params.sources
        );
        match self
            .paper_load_balancer
            .search(&params.query, params.n, &params.sources)
            .await
        {
            Ok(response) => Ok(serde_json::to_string_pretty(&response)
                .unwrap_or_else(|_| format!("{:?}", response))),
            Err(e) => Ok(format!("Paper search failed: {}", e)),
        }
    }

    /// Paper fetch tool - fetches paper details/content by identifier.
    #[tool(
        description = "Fetch academic paper details by identifier (DOI, arXiv ID, PubMed ID, PMCID, \
            Semantic Scholar ID, or URL). Provide one or more ID fields. \
            Tries the specified sources in order with fallback. \
            Available fetch sources: arxiv, semantic, sci_hub, pmc, pubmed."
    )]
    async fn paper_fetch(
        &self,
        Parameters(params): Parameters<PaperFetchParams>,
    ) -> Result<String, McpError> {
        tracing::info!(
            "paper_fetch called: id={}, sources={:?}",
            params.id,
            params.sources
        );
        match self
            .paper_load_balancer
            .fetch(&params.id, &params.sources)
            .await
        {
            Ok(response) => Ok(serde_json::to_string_pretty(&response)
                .unwrap_or_else(|_| format!("{:?}", response))),
            Err(e) => Ok(format!("Paper fetch failed: {}", e)),
        }
    }
}

impl Default for WebSearchMcpServer {
    fn default() -> Self {
        let config = Config::default();
        Self {
            tool_router: Self::tool_router(),
            load_balancer: ProviderLoadBalancer::from_config(&config)
                .expect("default config should work"),
            paper_load_balancer: PaperLoadBalancer::from_config(&config),
        }
    }
}

#[tool_handler]
impl ServerHandler for WebSearchMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
    }
}
