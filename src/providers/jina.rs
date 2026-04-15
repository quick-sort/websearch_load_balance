//! Jina Reader web fetch provider implementation.

use crate::error::WebSearchError;
use crate::providers::trait_def::{FetchResponse, SearchResponse, SearchResult, WebSearchProvider};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct JinaFetchResponse {
    data: JinaFetchData,
}

#[derive(Debug, Deserialize)]
struct JinaFetchData {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct JinaSearchResponse {
    #[serde(default)]
    data: Vec<JinaSearchResult>,
}

#[derive(Debug, Deserialize)]
struct JinaSearchResult {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    favicon: Option<String>,
}

/// Jina provider implementation (search requires API key, fetch key is optional).
pub struct JinaProvider {
    client: Client,
    base_url: String,
    api_key: String,
}

impl JinaProvider {
    pub fn new(base_url: String, api_key: String) -> Self {
        Self {
            client: Client::new(),
            base_url,
            api_key,
        }
    }
}

#[async_trait]
impl WebSearchProvider for JinaProvider {
    fn name(&self) -> &str {
        "jina"
    }

    async fn search(
        &self,
        query: &str,
        _max_results: u32,
    ) -> Result<SearchResponse, WebSearchError> {
        if self.api_key.is_empty() {
            return Err(WebSearchError::NotSupported(
                "Jina search requires an API key".to_string(),
            ));
        }

        let search_url = format!("{}/", self.base_url.replace("://r.", "://s."));

        let resp = self
            .client
            .get(&search_url)
            .header("Accept", "application/json")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("X-Respond-With", "no-content")
            .header("X-With-Favicons", "true")
            .query(&[("q", query)])
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16() as i32;
            let body = resp.text().await.unwrap_or_default();
            return Err(WebSearchError::ProviderError(status, body));
        }

        let response = resp.json::<JinaSearchResponse>().await?;

        let organic = response
            .data
            .into_iter()
            .map(|r| SearchResult {
                title: r.title.unwrap_or_default(),
                link: r.url.unwrap_or_default(),
                snippet: r.description.unwrap_or_default(),
                date: None,
                favicon: r.favicon,
            })
            .collect();

        Ok(SearchResponse {
            organic,
            related_searches: vec![],
        })
    }

    async fn fetch(&self, url: &str) -> Result<FetchResponse, WebSearchError> {
        let fetch_url = format!("{}/{}", self.base_url.trim_end_matches('/'), url);

        let mut req = self
            .client
            .get(&fetch_url)
            .header("Accept", "application/json")
            .header("X-Return-Format", "markdown");

        if !self.api_key.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", self.api_key));
        }

        let resp = req.send().await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16() as i32;
            let body = resp.text().await.unwrap_or_default();
            return Err(WebSearchError::ProviderError(status, body));
        }

        let response = resp.json::<JinaFetchResponse>().await?;

        Ok(FetchResponse {
            content: response.data.content.unwrap_or_default(),
            url: response.data.url.unwrap_or_else(|| url.to_string()),
            title: response.data.title,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_name() {
        let provider = JinaProvider::new("https://r.jina.ai".to_string(), String::new());
        assert_eq!(provider.name(), "jina");
    }

    #[tokio::test]
    #[ignore] // 需要网络访问
    async fn test_fetch_integration() {
        let api_key = crate::error::parse_api_key("JINA_API_KEYS");
        let provider = JinaProvider::new("https://r.jina.ai".to_string(), api_key);

        let result = provider.fetch("https://www.rust-lang.org/").await;
        assert!(result.is_ok(), "获取失败: {:?}", result);

        let response = result.unwrap();
        assert!(!response.content.is_empty(), "无内容");
    }
}
