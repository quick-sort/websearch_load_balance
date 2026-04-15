//! Brave Search provider implementation.

use crate::error::WebSearchError;
use crate::providers::trait_def::{SearchResponse, SearchResult, WebSearchProvider};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct BraveSearchResponse {
    #[serde(default)]
    web: Option<BraveWebResults>,
}

#[derive(Debug, Deserialize)]
struct BraveWebResults {
    #[serde(default)]
    results: Vec<BraveWebResult>,
}

#[derive(Debug, Deserialize)]
struct BraveWebResult {
    title: String,
    url: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    extra_snippets: Option<Vec<String>>,
}

/// Brave Search provider implementation.
pub struct BraveProvider {
    client: Client,
    base_url: String,
    api_key: String,
}

impl BraveProvider {
    pub fn new(base_url: String, api_key: String) -> Self {
        Self {
            client: Client::new(),
            base_url,
            api_key,
        }
    }
}

#[async_trait]
impl WebSearchProvider for BraveProvider {
    fn name(&self) -> &str {
        "brave"
    }

    async fn search(
        &self,
        query: &str,
        max_results: u32,
    ) -> Result<SearchResponse, WebSearchError> {
        let url = format!("{}/res/v1/web/search", self.base_url);
        let count = max_results.min(20);

        let resp = self
            .client
            .get(&url)
            .header("X-Subscription-Token", &self.api_key)
            .header("Accept", "application/json")
            .query(&[("q", query), ("count", &count.to_string())])
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16() as i32;
            let body = resp.text().await.unwrap_or_default();
            return Err(WebSearchError::ProviderError(status, body));
        }

        let response = resp.json::<BraveSearchResponse>().await?;
        let results = response.web.map(|w| w.results).unwrap_or_default();

        let organic: Vec<SearchResult> = results
            .into_iter()
            .map(|r| {
                let snippet = match (&r.description, &r.extra_snippets) {
                    (Some(desc), Some(extras)) => std::iter::once(desc.as_str())
                        .chain(extras.iter().map(|s| s.as_str()))
                        .collect::<Vec<_>>()
                        .join("\n"),
                    (Some(desc), None) => desc.clone(),
                    (None, Some(extras)) => extras.join("\n"),
                    (None, None) => String::new(),
                };
                let favicon = reqwest::Url::parse(&r.url)
                    .ok()
                    .and_then(|u| u.host_str().map(|h| h.to_string()))
                    .map(|domain| {
                        format!("https://www.google.com/s2/favicons?domain={domain}&sz=32")
                    });
                SearchResult {
                    title: r.title,
                    link: r.url,
                    snippet,
                    date: None,
                    favicon,
                }
            })
            .collect();

        Ok(SearchResponse {
            organic,
            related_searches: vec![],
        })
    }

    async fn fetch(&self, _url: &str) -> Result<crate::providers::FetchResponse, WebSearchError> {
        Err(WebSearchError::NotSupported(
            "Brave Search does not support web fetch".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_name() {
        let provider = BraveProvider::new(
            "https://api.search.brave.com".to_string(),
            "test-key".to_string(),
        );
        assert_eq!(provider.name(), "brave");
    }

    #[tokio::test]
    #[ignore] // 需要 BRAVE_API_KEYS 环境变量
    async fn test_search_integration() {
        let api_key = crate::error::parse_api_key("BRAVE_API_KEYS");
        if api_key.is_empty() {
            eprintln!("跳过: BRAVE_API_KEYS 未设置");
            return;
        }

        let provider = BraveProvider::new("https://api.search.brave.com".to_string(), api_key);

        let result = provider.search("Rust programming language", 5).await;
        assert!(result.is_ok(), "搜索失败: {:?}", result);

        let response = result.unwrap();
        assert!(!response.organic.is_empty(), "无搜索结果");

        let first = &response.organic[0];
        assert!(!first.title.is_empty());
        assert!(!first.link.is_empty());
        assert!(!first.snippet.is_empty());
    }
}
