//! Firecrawl web search and fetch provider implementation.

use crate::error::WebSearchError;
use crate::providers::trait_def::{FetchResponse, SearchResponse, SearchResult, WebSearchProvider};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;

/// Firecrawl search response.
#[derive(Debug, Deserialize)]
struct FirecrawlSearchResponse {
    success: bool,
    data: FirecrawlData,
}

/// Firecrawl data wrapper.
#[derive(Debug, Deserialize)]
struct FirecrawlData {
    #[serde(default)]
    web: Vec<FirecrawlResult>,
}

/// Individual Firecrawl search result.
#[derive(Debug, Deserialize)]
struct FirecrawlResult {
    url: String,
    title: String,
    #[serde(default)]
    description: Option<String>,
}

/// Firecrawl scrape response.
#[derive(Debug, Deserialize)]
struct FirecrawlScrapeResponse {
    success: bool,
    data: FirecrawlScrapeData,
}

/// Firecrawl scrape data.
#[derive(Debug, Deserialize)]
struct FirecrawlScrapeData {
    #[serde(default)]
    markdown: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    html: Option<String>,
    #[serde(default)]
    metadata: Option<FirecrawlMetadata>,
}

/// Firecrawl metadata.
#[derive(Debug, Deserialize)]
struct FirecrawlMetadata {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    description: Option<String>,
}

/// Firecrawl provider implementation.
pub struct FirecrawlProvider {
    client: Client,
    base_url: String,
    api_key: String,
}

impl FirecrawlProvider {
    /// Create a new Firecrawl provider with the given API key.
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
impl WebSearchProvider for FirecrawlProvider {
    fn name(&self) -> &str {
        "firecrawl"
    }

    async fn search(
        &self,
        query: &str,
        max_results: u32,
    ) -> Result<SearchResponse, WebSearchError> {
        let url = format!("{}/v2/search", self.base_url);

        let resp = self
            .client
            .post(&url)
            .header("Authorization", self.auth_header())
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "query": query,
                "limit": max_results
            }))
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status().as_u16() as i32;
            let body = resp.text().await.unwrap_or_default();
            return Err(WebSearchError::ProviderError(status, body));
        }
        let response = resp.json::<FirecrawlSearchResponse>().await?;

        if !response.success {
            return Err(WebSearchError::ProviderError(
                500,
                "Firecrawl search failed".to_string(),
            ));
        }

        // Firecrawl returns results in data.web array
        let results: Vec<SearchResult> = response
            .data
            .web
            .into_iter()
            .map(|r| {
                SearchResult {
                    title: r.title,
                    link: r.url,
                    snippet: r.description.unwrap_or_default(),
                    date: None,    // Firecrawl doesn't provide date
                    favicon: None, // Firecrawl doesn't provide favicon
                }
            })
            .collect();

        Ok(SearchResponse {
            organic: results,
            related_searches: Vec::new(), // Firecrawl doesn't provide related searches
        })
    }

    async fn fetch(&self, url: &str) -> Result<FetchResponse, WebSearchError> {
        let fetch_url = format!("{}/v2/scrape", self.base_url);

        let resp = self
            .client
            .post(&fetch_url)
            .header("Authorization", self.auth_header())
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "url": url,
                "formats": ["markdown"]
            }))
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status().as_u16() as i32;
            let body = resp.text().await.unwrap_or_default();
            return Err(WebSearchError::ProviderError(status, body));
        }
        let response = resp.json::<FirecrawlScrapeResponse>().await?;

        if !response.success {
            return Err(WebSearchError::ProviderError(
                500,
                "Firecrawl scrape failed".to_string(),
            ));
        }

        let content = response.data.markdown.unwrap_or_default();
        let title = response.data.metadata.and_then(|m| m.title);

        Ok(FetchResponse {
            content,
            url: url.to_string(),
            title,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_name() {
        let provider = FirecrawlProvider::new(
            "https://api.firecrawl.dev".to_string(),
            "test-key".to_string(),
        );
        assert_eq!(provider.name(), "firecrawl");
    }

    #[test]
    fn test_auth_header() {
        let provider = FirecrawlProvider::new(
            "https://api.firecrawl.dev".to_string(),
            "fc-key-abc123".to_string(),
        );
        assert_eq!(provider.auth_header(), "Bearer fc-key-abc123");
    }
}
