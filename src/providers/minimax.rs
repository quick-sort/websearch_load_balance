//! MiniMax (Coding Plan) web search provider implementation.

use crate::error::WebSearchError;
use crate::providers::trait_def::{FetchResponse, RelatedSearch, SearchResponse, SearchResult, WebSearchProvider};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;

/// MiniMax API search response.
#[derive(Debug, Deserialize)]
struct MiniMaxSearchResponse {
    #[serde(default)]
    organic: Vec<MiniMaxResult>,
    #[serde(default)]
    related_searches: Vec<MiniMaxRelatedSearch>,
    base_resp: MiniMaxBaseResp,
}

/// Individual MiniMax search result.
#[derive(Debug, Deserialize)]
struct MiniMaxResult {
    title: String,
    link: String,
    snippet: String,
    #[serde(default)]
    date: Option<String>,
}

/// MiniMax related search.
#[derive(Debug, Deserialize)]
struct MiniMaxRelatedSearch {
    query: String,
}

/// MiniMax API base response.
#[derive(Debug, Deserialize)]
struct MiniMaxBaseResp {
    #[serde(rename = "status_code")]
    status_code: i32,
    #[serde(rename = "status_msg")]
    status_msg: String,
}

/// MiniMax provider implementation.
///
/// Note: MiniMax Coding Plan does not have a web fetch API,
/// so fetch() will return NotSupported error.
pub struct MiniMaxProvider {
    client: Client,
    base_url: String,
    api_key: String,
}

impl MiniMaxProvider {
    /// Create a new MiniMax provider with the given API key.
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

    fn check_response(&self, response: &MiniMaxSearchResponse) -> Result<(), WebSearchError> {
        if response.base_resp.status_code != 0 {
            match response.base_resp.status_code {
                1004 => {
                    return Err(WebSearchError::AuthError(response.base_resp.status_msg.clone()));
                }
                2038 => {
                    return Err(WebSearchError::AuthError(format!(
                        "{} (need real-name verification)",
                        response.base_resp.status_msg
                    )));
                }
                _ => {
                    return Err(WebSearchError::ProviderError(
                        response.base_resp.status_code,
                        response.base_resp.status_msg.clone(),
                    ));
                }
            }
        }
        Ok(())
    }
}

#[async_trait]
impl WebSearchProvider for MiniMaxProvider {
    fn name(&self) -> &str {
        "minimax"
    }

    async fn search(&self, query: &str, max_results: u32) -> Result<SearchResponse, WebSearchError> {
        let url = format!("{}/v1/coding_plan/search", self.base_url);

        let response = self.client
            .post(&url)
            .header("Authorization", self.auth_header())
            .header("Content-Type", "application/json")
            .header("MM-API-Source", "websearch-load-balance")
            .json(&serde_json::json!({
                "q": query,
                "num": max_results
            }))
            .send()
            .await?
            .json::<MiniMaxSearchResponse>()
            .await?;

        self.check_response(&response)?;

        Ok(SearchResponse {
            organic: response.organic.into_iter().map(|r| SearchResult {
                title: r.title,
                link: r.link,
                snippet: r.snippet,
                date: r.date,
            }).collect(),
            related_searches: response.related_searches.into_iter()
                .map(|rs| RelatedSearch { query: rs.query })
                .collect(),
        })
    }

    async fn fetch(&self, _url: &str) -> Result<FetchResponse, WebSearchError> {
        // MiniMax Coding Plan does not have a fetch/reader API
        Err(WebSearchError::NotSupported(
            "MiniMax Coding Plan does not support web fetch".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_name() {
        let provider = MiniMaxProvider::new(
            "https://api.minimaxi.com".to_string(),
            "test-key".to_string(),
        );
        assert_eq!(provider.name(), "minimax");
    }

    #[test]
    fn test_auth_header() {
        let provider = MiniMaxProvider::new(
            "https://api.minimaxi.com".to_string(),
            "mmx-abc123".to_string(),
        );
        assert_eq!(provider.auth_header(), "Bearer mmx-abc123");
    }
}