//! ZhiPu (智普) web search provider implementation.

use crate::error::WebSearchError;
use crate::providers::trait_def::{FetchResponse, RelatedSearch, SearchResponse, SearchResult, WebSearchProvider};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;

/// ZhiPu API search response.
#[derive(Debug, Deserialize)]
struct ZhiPuSearchResponse {
    id: Option<String>,
    #[serde(default)]
    results: Vec<ZhiPuResult>,
    #[serde(default)]
    search_intent: Vec<ZhiPuSearchIntent>,
}

/// Individual ZhiPu search result.
#[derive(Debug, Deserialize)]
struct ZhiPuResult {
    title: String,
    url: String,
    content: String,
    #[serde(default)]
    media: Option<String>,
    #[serde(default)]
    icon: Option<String>,
    #[serde(default)]
    publish_date: Option<String>,
}

/// ZhiPu search intent.
#[derive(Debug, Deserialize)]
struct ZhiPuSearchIntent {
    query: String,
    #[serde(rename = "intent")]
    intent_type: String,
    keywords: String,
}

/// ZhiPu API reader (fetch) response.
#[derive(Debug, Deserialize)]
struct ZhiPuReaderResponse {
    id: Option<String>,
    #[serde(default)]
    reader_result: ZhiPuReaderResult,
}

/// ZhiPu reader result.
#[derive(Debug, Deserialize, Default)]
struct ZhiPuReaderResult {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    url: Option<String>,
}

/// ZhiPu provider implementation.
pub struct ZhiPuProvider {
    client: Client,
    base_url: String,
    api_key: String,
}

impl ZhiPuProvider {
    /// Create a new ZhiPu provider with the given API key.
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
impl WebSearchProvider for ZhiPuProvider {
    fn name(&self) -> &str {
        "zhipu"
    }

    async fn search(&self, query: &str, max_results: u32) -> Result<SearchResponse, WebSearchError> {
        let url = format!("{}/api/paas/v4/web_search", self.base_url);

        let response = self.client
            .post(&url)
            .header("Authorization", self.auth_header())
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "search_query": query,
                "search_engine": "search_std",
                "count": max_results
            }))
            .send()
            .await?
            .json::<ZhiPuSearchResponse>()
            .await?;

        Ok(SearchResponse {
            organic: response.results.into_iter().map(|r| SearchResult {
                title: r.title,
                link: r.url,
                snippet: r.content,
                date: r.publish_date,
            }).collect(),
            // ZhiPu doesn't have related searches in the same format, use search_intent as fallback
            related_searches: response.search_intent.into_iter()
                .map(|si| RelatedSearch { query: si.query })
                .collect(),
        })
    }

    async fn fetch(&self, url: &str) -> Result<FetchResponse, WebSearchError> {
        let fetch_url = format!("{}/api/paas/v4/reader", self.base_url);

        let response = self.client
            .post(&fetch_url)
            .header("Authorization", self.auth_header())
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "url": url,
                "return_format": "markdown"
            }))
            .send()
            .await?
            .json::<ZhiPuReaderResponse>()
            .await?;

        let content = response.reader_result.content.unwrap_or_default();

        Ok(FetchResponse {
            content,
            url: url.to_string(),
            title: response.reader_result.title,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_name() {
        let provider = ZhiPuProvider::new(
            "https://open.bigmodel.cn".to_string(),
            "test-key".to_string(),
        );
        assert_eq!(provider.name(), "zhipu");
    }

    #[test]
    fn test_auth_header() {
        let provider = ZhiPuProvider::new(
            "https://open.bigmodel.cn".to_string(),
            "zhipu-abc123".to_string(),
        );
        assert_eq!(provider.auth_header(), "Bearer zhipu-abc123");
    }
}