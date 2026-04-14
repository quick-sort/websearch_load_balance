//! Serper web search and fetch provider implementation.

use crate::error::WebSearchError;
use crate::providers::trait_def::{FetchResponse, SearchResponse, SearchResult, WebSearchProvider};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;

/// Serper search response.
#[derive(Debug, Deserialize)]
struct SerperSearchResponse {
    #[serde(default)]
    organic: Vec<SerperResult>,
    #[serde(default, alias = "relatedSearches")]
    related_searches: Vec<SerperRelatedSearch>,
}

/// Serper organic result.
#[derive(Debug, Deserialize)]
struct SerperResult {
    title: String,
    link: String,
    #[serde(default)]
    snippet: Option<String>,
}

/// Serper related search.
#[derive(Debug, Deserialize)]
struct SerperRelatedSearch {
    query: String,
}

/// Serper scrape response.
#[derive(Debug, Deserialize)]
struct SerperScrapeResponse {
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    metadata: Option<SerperMetadata>,
}

/// Serper metadata.
#[derive(Debug, Deserialize)]
struct SerperMetadata {
    #[serde(default)]
    title: Option<String>,
}

/// Serper provider implementation.
pub struct SerperProvider {
    client: Client,
    base_url: String,
    api_key: String,
}

impl SerperProvider {
    /// Create a new Serper provider with the given API key.
    pub fn new(base_url: String, api_key: String) -> Self {
        Self {
            client: Client::new(),
            base_url,
            api_key,
        }
    }

    fn auth_header(&self) -> String {
        self.api_key.clone()
    }
}

#[async_trait]
impl WebSearchProvider for SerperProvider {
    fn name(&self) -> &str {
        "serper"
    }

    async fn search(&self, query: &str, max_results: u32) -> Result<SearchResponse, WebSearchError> {
        let url = format!("{}/search", self.base_url);

        let response = self.client
            .post(&url)
            .header("X-API-KEY", self.auth_header())
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "q": query,
                "num": max_results
            }))
            .send()
            .await?
            .json::<SerperSearchResponse>()
            .await?;

        let organic: Vec<SearchResult> = response.organic.into_iter().map(|r| {
            SearchResult {
                title: r.title,
                link: r.link,
                snippet: r.snippet.unwrap_or_default(),
                date: None, // Serper doesn't provide date
                favicon: None, // Serper doesn't provide favicon
            }
        }).collect();

        let related_searches = response.related_searches.into_iter()
            .map(|rs| crate::providers::RelatedSearch { query: rs.query })
            .collect();

        Ok(SearchResponse {
            organic,
            related_searches,
        })
    }

    async fn fetch(&self, url: &str) -> Result<FetchResponse, WebSearchError> {
        let scrape_url = format!("{}/scrape", self.base_url);

        let response = self.client
            .post(&scrape_url)
            .header("X-API-KEY", self.auth_header())
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "url": url
            }))
            .send()
            .await?
            .json::<SerperScrapeResponse>()
            .await?;

        let content = response.text.unwrap_or_default();
        let title = response.metadata.and_then(|m| m.title);

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
        let provider = SerperProvider::new(
            "https://google.serper.dev".to_string(),
            "test-key".to_string(),
        );
        assert_eq!(provider.name(), "serper");
    }

    #[test]
    fn test_auth_header() {
        let provider = SerperProvider::new(
            "https://google.serper.dev".to_string(),
            "serper-key-abc123".to_string(),
        );
        assert_eq!(provider.auth_header(), "serper-key-abc123");
    }
}