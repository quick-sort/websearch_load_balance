//! Tavily web search provider implementation.

use crate::error::WebSearchError;
use crate::providers::trait_def::{FetchResponse, RelatedSearch, SearchResponse, SearchResult, WebSearchProvider};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;

/// Tavily API search response.
#[derive(Debug, Deserialize)]
struct TavilySearchResponse {
    results: Vec<TavilyResult>,
    #[serde(default)]
    related_queries: Vec<TavilyRelatedQuery>,
}

/// Individual Tavily search result.
#[derive(Debug, Deserialize)]
struct TavilyResult {
    title: String,
    url: String,
    content: String,
    #[serde(default)]
    published_date: Option<String>,
}

/// Tavily related query.
#[derive(Debug, Deserialize)]
struct TavilyRelatedQuery {
    #[serde(rename = "query")]
    query: String,
}

/// Tavily API extract response.
#[derive(Debug, Deserialize)]
struct TavilyExtractResponse {
    results: Vec<TavilyExtractResult>,
    #[serde(default)]
    failed_results: Vec<TavilyFailedResult>,
}

/// Tavily extract result.
#[derive(Debug, Deserialize)]
struct TavilyExtractResult {
    url: String,
    content: String,
    #[serde(default)]
    raw_content: Option<String>,
    #[serde(default)]
    images: Vec<String>,
}

/// Tavily failed extract result.
#[derive(Debug, Deserialize)]
struct TavilyFailedResult {
    url: String,
    error: String,
}

/// Tavily provider with API key for load balancing.
pub struct TavilyProvider {
    client: Client,
    base_url: String,
    api_key: String,
}

impl TavilyProvider {
    /// Create a new Tavily provider with the given API key.
    pub fn new(base_url: String, api_key: String) -> Self {
        Self {
            client: Client::new(),
            base_url,
            api_key,
        }
    }

    /// Get the API key.
    pub fn api_key(&self) -> &str {
        &self.api_key
    }

    fn auth_header(&self) -> String {
        format!("Bearer {}", self.api_key)
    }
}

#[async_trait]
impl WebSearchProvider for TavilyProvider {
    fn name(&self) -> &str {
        "tavily"
    }

    async fn search(&self, query: &str, max_results: u32) -> Result<SearchResponse, WebSearchError> {
        let url = format!("{}/search", self.base_url);

        let response = self.client
            .post(&url)
            .header("Authorization", self.auth_header())
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "query": query,
                "max_results": max_results,
                "include_answers": true,
            }))
            .send()
            .await?
            .json::<TavilySearchResponse>()
            .await?;

        Ok(SearchResponse {
            organic: response.results.into_iter().map(|r| SearchResult {
                title: r.title,
                link: r.url,
                snippet: r.content,
                date: r.published_date,
            }).collect(),
            related_searches: response.related_queries.into_iter()
                .map(|rq| RelatedSearch { query: rq.query })
                .collect(),
        })
    }

    async fn fetch(&self, url: &str) -> Result<FetchResponse, WebSearchError> {
        let fetch_url = format!("{}/extract", self.base_url);

        let response = self.client
            .post(&fetch_url)
            .header("Authorization", self.auth_header())
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "urls": [url],
                "extract_depth": "basic"
            }))
            .send()
            .await?
            .json::<TavilyExtractResponse>()
            .await?;

        // Find the result for the requested URL
        for result in response.results {
            if result.url == url {
                return Ok(FetchResponse {
                    content: result.content,
                    url: url.to_string(),
                    title: None,
                });
            }
        }

        // Check if there was a failure
        for failed in response.failed_results {
            if failed.url == url {
                return Err(WebSearchError::ParseError(failed.error));
            }
        }

        Err(WebSearchError::ParseError(format!("No results for URL: {}", url)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_name() {
        let provider = TavilyProvider::new(
            "https://api.tavily.com".to_string(),
            "test-key".to_string(),
        );
        assert_eq!(provider.name(), "tavily");
    }

    #[test]
    fn test_auth_header() {
        let provider = TavilyProvider::new(
            "https://api.tavily.com".to_string(),
            "tvly-abc123".to_string(),
        );
        assert_eq!(provider.auth_header(), "Bearer tvly-abc123");
    }
}