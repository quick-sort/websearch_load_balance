//! SerpAPI Google search provider implementation.

use crate::error::WebSearchError;
use crate::providers::trait_def::{SearchResponse, SearchResult, WebSearchProvider};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;

/// SerpAPI search response.
#[derive(Debug, Deserialize)]
struct SerpApiResponse {
    #[serde(default)]
    organic_results: Vec<SerpApiResult>,
    #[serde(default)]
    search_information: Option<SerpApiSearchInfo>,
    #[serde(default)]
    related_questions: Vec<SerpApiRelatedQuestion>,
}

/// SerpAPI search information.
#[derive(Debug, Deserialize)]
struct SerpApiSearchInfo {
    #[serde(default)]
    query_displayed: Option<String>,
}

/// SerpAPI organic result.
#[derive(Debug, Deserialize)]
struct SerpApiResult {
    title: String,
    link: String,
    #[serde(default)]
    snippet: Option<String>,
    #[serde(default)]
    date: Option<String>,
    #[serde(default)]
    displayed_link: Option<String>,
    #[serde(default)]
    favicon: Option<String>,
}

/// SerpAPI related question.
#[derive(Debug, Deserialize)]
struct SerpApiRelatedQuestion {
    question: String,
}

/// SerpAPI provider implementation.
pub struct SerpApiProvider {
    client: Client,
    base_url: String,
    api_key: String,
}

impl SerpApiProvider {
    /// Create a new SerpAPI provider with the given API key.
    pub fn new(base_url: String, api_key: String) -> Self {
        Self {
            client: Client::new(),
            base_url,
            api_key,
        }
    }
}

#[async_trait]
impl WebSearchProvider for SerpApiProvider {
    fn name(&self) -> &str {
        "serpapi"
    }

    async fn search(&self, query: &str, max_results: u32) -> Result<SearchResponse, WebSearchError> {
        let url = format!("{}/search", self.base_url);

        let response = self.client
            .get(&url)
            .query(&[
                ("engine", "google"),
                ("q", query),
                ("hl", "en"),
                ("gl", "us"),
                ("num", &max_results.to_string()),
                ("api_key", &self.api_key),
            ])
            .send()
            .await?
            .json::<SerpApiResponse>()
            .await?;

        // Build search results
        let organic: Vec<SearchResult> = response.organic_results.into_iter().map(|r| {
            SearchResult {
                title: r.title,
                link: r.link,
                snippet: r.snippet.unwrap_or_default(),
                date: r.date,
                favicon: r.favicon,
            }
        }).collect();

        // Build related searches from related_questions
        let related_searches = response.related_questions.into_iter()
            .map(|rq| crate::providers::RelatedSearch { query: rq.question })
            .collect();

        Ok(SearchResponse {
            organic,
            related_searches,
        })
    }

    async fn fetch(&self, _url: &str) -> Result<crate::providers::FetchResponse, WebSearchError> {
        Err(WebSearchError::NotSupported(
            "SerpAPI does not support web fetch".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_name() {
        let provider = SerpApiProvider::new(
            "https://serpapi.com".to_string(),
            "test-key".to_string(),
        );
        assert_eq!(provider.name(), "serpapi");
    }
}