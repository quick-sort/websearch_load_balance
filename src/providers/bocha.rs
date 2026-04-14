//! Bocha (博查) web search provider implementation.

use crate::error::WebSearchError;
use crate::providers::trait_def::{FetchResponse, SearchResponse, SearchResult, WebSearchProvider};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;

/// Bocha API search response.
#[derive(Debug, Deserialize)]
struct BochaSearchResponse {
    code: i32,
    msg: Option<String>,
    data: BochaData,
}

/// Bocha data wrapper.
#[derive(Debug, Deserialize)]
struct BochaData {
    #[serde(rename = "webPages", default)]
    web_pages: Option<BochaWebPages>,
}

/// Bocha web pages result.
#[derive(Debug, Deserialize)]
struct BochaWebPages {
    #[serde(default)]
    value: Vec<BochaResult>,
}

/// Individual Bocha search result.
#[derive(Debug, Deserialize)]
struct BochaResult {
    name: String,
    url: String,
    #[serde(default)]
    snippet: Option<String>,
    #[serde(default)]
    summary: Option<String>,
    #[serde(rename = "siteName", default)]
    site_name: Option<String>,
    #[serde(rename = "siteIcon", default)]
    site_icon: Option<String>,
    #[serde(rename = "datePublished", default)]
    date_published: Option<String>,
}

/// Bocha provider implementation.
pub struct BochaProvider {
    client: Client,
    base_url: String,
    api_key: String,
}

impl BochaProvider {
    /// Create a new Bocha provider with the given API key.
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

    fn check_response(&self, response: &BochaSearchResponse) -> Result<(), WebSearchError> {
        if response.code != 200 {
            return Err(WebSearchError::ProviderError(
                response.code,
                response.msg.clone().unwrap_or_default(),
            ));
        }
        Ok(())
    }
}

#[async_trait]
impl WebSearchProvider for BochaProvider {
    fn name(&self) -> &str {
        "bocha"
    }

    async fn search(&self, query: &str, max_results: u32) -> Result<SearchResponse, WebSearchError> {
        let url = format!("{}/v1/web-search", self.base_url);

        let response = self.client
            .post(&url)
            .header("Authorization", self.auth_header())
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "query": query,
                "summary": true,
                "freshness": "noLimit",
                "count": max_results
            }))
            .send()
            .await?
            .json::<BochaSearchResponse>()
            .await?;

        self.check_response(&response)?;

        let results = response.data.web_pages
            .map(|wp| wp.value)
            .unwrap_or_default();

        Ok(SearchResponse {
            organic: results.into_iter().map(|r| {
                // Use summary if available, otherwise snippet
                let snippet = r.summary.unwrap_or(r.snippet.unwrap_or_default());
                SearchResult {
                    title: r.name,
                    link: r.url,
                    snippet,
                    date: r.date_published,
                    favicon: r.site_icon,
                }
            }).collect(),
            related_searches: Vec::new(), // Bocha doesn't provide related searches
        })
    }

    async fn fetch(&self, _url: &str) -> Result<FetchResponse, WebSearchError> {
        // Bocha API doesn't provide a fetch/reader endpoint
        Err(WebSearchError::NotSupported(
            "Bocha does not support web fetch".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_name() {
        let provider = BochaProvider::new(
            "https://api.bocha.cn".to_string(),
            "test-key".to_string(),
        );
        assert_eq!(provider.name(), "bocha");
    }

    #[test]
    fn test_auth_header() {
        let provider = BochaProvider::new(
            "https://api.bocha.cn".to_string(),
            "bocha-key-abc123".to_string(),
        );
        assert_eq!(provider.auth_header(), "Bearer bocha-key-abc123");
    }

    #[tokio::test]
    #[ignore] // 需要 BOCHA_API_KEY 环境变量
    async fn test_search_integration() {
        let api_key = std::env::var("BOCHA_API_KEY").unwrap_or_default();
        if api_key.is_empty() {
            eprintln!("跳过: BOCHA_API_KEY 未设置");
            return;
        }

        let provider = BochaProvider::new(
            "https://api.bocha.cn".to_string(),
            api_key,
        );

        let result = provider.search("阿里巴巴ESG报告", 5).await;
        assert!(result.is_ok(), "搜索失败: {:?}", result);

        let response = result.unwrap();
        assert!(!response.organic.is_empty(), "无搜索结果");

        // 验证返回结构
        let first = &response.organic[0];
        assert!(!first.title.is_empty());
        assert!(!first.link.is_empty());
    }
}