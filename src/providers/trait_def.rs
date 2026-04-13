//! Provider trait definition for web search services.

use crate::error::WebSearchError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Result type for search operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResponse {
    /// Organic search results.
    pub organic: Vec<SearchResult>,
    /// Related search queries.
    #[serde(default)]
    pub related_searches: Vec<RelatedSearch>,
}

/// Individual search result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// Title of the search result.
    pub title: String,
    /// URL link to the result.
    pub link: String,
    /// Brief description/excerpt.
    pub snippet: String,
    /// Publication date (if available).
    #[serde(default, rename = "date")]
    pub date: Option<String>,
}

/// Related search query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelatedSearch {
    /// The related query string.
    pub query: String,
}

/// Result type for fetch operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchResponse {
    /// Extracted content (markdown).
    pub content: String,
    /// Original URL.
    pub url: String,
    /// Page title (if available).
    #[serde(default)]
    pub title: Option<String>,
}

impl fmt::Display for SearchResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SearchResponse(results: {})", self.organic.len())
    }
}

impl fmt::Display for FetchResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FetchResponse(url: {})", self.url)
    }
}

/// Trait for web search providers.
///
/// Implement this trait to add support for a new search provider.
#[async_trait]
pub trait WebSearchProvider: Send + Sync {
    /// Get the provider name.
    fn name(&self) -> &str;

    /// Search the web with a query.
    ///
    /// # Arguments
    /// * `query` - The search query string
    /// * `max_results` - Maximum number of results to return
    ///
    /// # Returns
    /// A `SearchResponse` containing search results and related queries.
    async fn search(&self, query: &str, max_results: u32) -> Result<SearchResponse, WebSearchError>;

    /// Fetch and extract content from a URL.
    ///
    /// # Arguments
    /// * `url` - The URL to fetch content from
    ///
    /// # Returns
    /// A `FetchResponse` containing the extracted content.
    async fn fetch(&self, url: &str) -> Result<FetchResponse, WebSearchError>;

    /// Validate that the API key works.
    ///
    /// Default implementation performs a test search.
    async fn validate_key(&self) -> Result<bool, WebSearchError> {
        match self.search("test", 1).await {
            Ok(_) => Ok(true),
            Err(e) if e.is_auth_error() => Ok(false),
            Err(_) => Ok(false),
        }
    }
}