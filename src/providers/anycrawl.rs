//! AnyCrawl web fetch provider implementation.

use crate::error::WebSearchError;
use crate::providers::trait_def::{FetchResponse, SearchResponse, WebSearchProvider};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;

/// AnyCrawl scrape response.
#[derive(Debug, Deserialize)]
struct AnyCrawlResponse {
    success: bool,
    data: Option<AnyCrawlData>,
}

/// AnyCrawl data wrapper.
#[derive(Debug, Deserialize)]
struct AnyCrawlData {
    #[serde(default)]
    markdown: Option<String>,
    #[serde(default)]
    status: Option<String>,
}

/// AnyCrawl provider implementation (fetch/scrape only).
pub struct AnycrawlProvider {
    client: Client,
    base_url: String,
    api_key: String,
}

impl AnycrawlProvider {
    /// Create a new AnyCrawl provider with the given API key.
    pub fn new(base_url: String, api_key: String) -> Self {
        Self {
            client: Client::new(),
            base_url,
            api_key,
        }
    }

    fn auth_header(&self) -> String {
        format!("Bearer {}", self.api_key)
    }
}

#[async_trait]
impl WebSearchProvider for AnycrawlProvider {
    fn name(&self) -> &str {
        "anycrawl"
    }

    async fn search(&self, _query: &str, _max_results: u32) -> Result<SearchResponse, WebSearchError> {
        Err(WebSearchError::NotSupported(
            "AnyCrawl does not support web search".to_string(),
        ))
    }

    async fn fetch(&self, url: &str) -> Result<FetchResponse, WebSearchError> {
        let fetch_url = format!("{}/v1/scrape", self.base_url);

        let response = self.client
            .post(&fetch_url)
            .header("Authorization", self.auth_header())
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "url": url,
                "formats": ["markdown"],
                "engine": "playwright",
                "wait_for": 2000
            }))
            .send()
            .await?
            .json::<AnyCrawlResponse>()
            .await?;

        match response.data {
            Some(data) if data.status.as_deref() == Some("completed") => {
                let content = data.markdown.unwrap_or_default();
                Ok(FetchResponse {
                    content,
                    url: url.to_string(),
                    title: None,
                })
            }
            Some(data) => Err(WebSearchError::ProviderError(
                500,
                format!("AnyCrawl scrape status: {:?}", data.status),
            )),
            None => Err(WebSearchError::ProviderError(
                500,
                "AnyCrawl returned no data".to_string(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_name() {
        let provider = AnycrawlProvider::new(
            "https://api.anycrawl.dev".to_string(),
            "test-key".to_string(),
        );
        assert_eq!(provider.name(), "anycrawl");
    }

    #[test]
    fn test_auth_header() {
        let provider = AnycrawlProvider::new(
            "https://api.anycrawl.dev".to_string(),
            "ac-key-abc123".to_string(),
        );
        assert_eq!(provider.auth_header(), "Bearer ac-key-abc123");
    }
}