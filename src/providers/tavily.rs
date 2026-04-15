//! Tavily web search provider implementation.

use crate::error::WebSearchError;
use crate::providers::trait_def::{
    FetchResponse, RelatedSearch, SearchResponse, SearchResult, WebSearchProvider,
};
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
    #[serde(default)]
    favicon: Option<String>,
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
    #[allow(dead_code)]
    url: String,
    #[serde(alias = "content")] // fallback if raw_content not present
    raw_content: String,
    #[serde(default)]
    #[allow(dead_code)]
    images: Vec<String>,
}

/// Tavily failed extract result.
#[derive(Debug, Deserialize)]
struct TavilyFailedResult {
    #[allow(dead_code)]
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

    async fn search(
        &self,
        query: &str,
        max_results: u32,
    ) -> Result<SearchResponse, WebSearchError> {
        let url = format!("{}/search", self.base_url);

        let resp = self
            .client
            .post(&url)
            .header("Authorization", self.auth_header())
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "query": query,
                "max_results": max_results,
                "include_answers": true,
                "include_favicon": true,
            }))
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16() as i32;
            let body = resp.text().await.unwrap_or_default();
            return Err(WebSearchError::ProviderError(status, body));
        }

        let response = resp.json::<TavilySearchResponse>().await?;

        Ok(SearchResponse {
            organic: response
                .results
                .into_iter()
                .map(|r| SearchResult {
                    title: r.title,
                    link: r.url,
                    snippet: r.content,
                    date: r.published_date,
                    favicon: r.favicon,
                })
                .collect(),
            related_searches: response
                .related_queries
                .into_iter()
                .map(|rq| RelatedSearch { query: rq.query })
                .collect(),
        })
    }

    async fn fetch(&self, url: &str) -> Result<FetchResponse, WebSearchError> {
        let fetch_url = format!("{}/extract", self.base_url);

        let resp = self
            .client
            .post(&fetch_url)
            .header("Authorization", self.auth_header())
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "urls": [url],
                "extract_depth": "basic"
            }))
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16() as i32;
            let body = resp.text().await.unwrap_or_default();
            return Err(WebSearchError::ProviderError(status, body));
        }

        let response = resp.json::<TavilyExtractResponse>().await?;

        // Take the first successful result (URL may be normalized by Tavily)
        if let Some(result) = response.results.into_iter().next() {
            return Ok(FetchResponse {
                content: result.raw_content,
                url: url.to_string(),
                title: None,
            });
        }

        // Check if there was a failure
        if let Some(failed) = response.failed_results.into_iter().next() {
            return Err(WebSearchError::ParseError(failed.error));
        }

        Err(WebSearchError::ParseError(format!(
            "No results for URL: {}",
            url
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_name() {
        let provider =
            TavilyProvider::new("https://api.tavily.com".to_string(), "test-key".to_string());
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

    #[tokio::test]
    #[ignore] // 需要 TAVILY_API_KEYS 环境变量
    async fn test_search_integration() {
        let api_key = crate::error::parse_api_key("TAVILY_API_KEYS");
        if api_key.is_empty() {
            eprintln!("跳过: TAVILY_API_KEYS 未设置");
            return;
        }

        let provider = TavilyProvider::new("https://api.tavily.com".to_string(), api_key);

        let result = provider.search("Rust programming language", 5).await;
        assert!(result.is_ok(), "搜索失败: {:?}", result);

        let response = result.unwrap();
        assert!(!response.organic.is_empty(), "无搜索结果");
        assert_eq!(response.organic.len(), 5);

        // 验证返回结构
        let first = &response.organic[0];
        assert!(!first.title.is_empty());
        assert!(!first.link.is_empty());
        assert!(!first.snippet.is_empty());
    }

    #[tokio::test]
    #[ignore] // 需要 TAVILY_API_KEY 环境变量
    async fn test_fetch_integration() {
        let api_key = crate::error::parse_api_key("TAVILY_API_KEYS");
        if api_key.is_empty() {
            eprintln!("跳过: TAVILY_API_KEYS 未设置");
            return;
        }

        let provider = TavilyProvider::new("https://api.tavily.com".to_string(), api_key);

        let result = provider.fetch("https://www.rust-lang.org/").await;
        assert!(result.is_ok(), "获取失败: {:?}", result);

        let response = result.unwrap();
        assert!(!response.content.is_empty(), "无内容");
        assert!(response.content.contains("Rust"));
    }
}
