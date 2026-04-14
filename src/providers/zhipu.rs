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
    #[serde(default, alias = "search_result")]
    results: Vec<ZhiPuResult>,
    #[serde(default)]
    search_intent: Vec<ZhiPuSearchIntent>,
}

/// Individual ZhiPu search result.
#[derive(Debug, Deserialize)]
struct ZhiPuResult {
    title: String,
    #[serde(alias = "url", alias = "link", default)]
    link: Option<String>,
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
    /// API 变体: "standard" (通用) 或 "coding" (Coding 套餐)
    api_variant: String,
}

impl ZhiPuProvider {
    /// Create a new ZhiPu provider with the given API key.
    pub fn new(base_url: String, api_key: String) -> Self {
        Self {
            client: Client::new(),
            base_url,
            api_key,
            api_variant: "standard".to_string(),
        }
    }

    /// Create a new ZhiPu provider with API variant.
    pub fn with_variant(base_url: String, api_key: String, api_variant: &str) -> Self {
        Self {
            client: Client::new(),
            base_url,
            api_key,
            api_variant: api_variant.to_string(),
        }
    }
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
        // 根据 api_variant 选择端点路径
        // standard: /api/paas/v4/web_search
        // coding: /api/coding/paas/v4/web_search
        let api_path = match self.api_variant.as_str() {
            "coding" => "/api/coding/paas/v4",
            _ => "/api/paas/v4",
        };
        let url = format!("{}{}/web_search", self.base_url, api_path);

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
            organic: response.results.into_iter().filter_map(|r| {
                let link = r.link.unwrap_or_default();
                if link.is_empty() { return None; }
                Some(SearchResult {
                    title: r.title,
                    link,
                    snippet: r.content,
                    date: r.publish_date,
                    favicon: r.icon.clone(),
                })
            }).collect(),
            related_searches: response.search_intent.into_iter()
                .map(|si| RelatedSearch { query: si.query })
                .collect(),
        })
    }

    async fn fetch(&self, url: &str) -> Result<FetchResponse, WebSearchError> {
        let api_path = match self.api_variant.as_str() {
            "coding" => "/api/coding/paas/v4",
            _ => "/api/paas/v4",
        };
        let fetch_url = format!("{}{}/reader", self.base_url, api_path);

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

    #[tokio::test]
    #[ignore] // 需要 GLM_API_KEY 环境变量
    async fn test_search_integration() {
        let api_key = std::env::var("GLM_API_KEY").unwrap_or_default();
        if api_key.is_empty() {
            eprintln!("跳过: GLM_API_KEY 未设置");
            return;
        }

        let provider = ZhiPuProvider::new(
            "https://open.bigmodel.cn".to_string(),
            api_key,
        );

        let result = provider.search("Rust 编程语言", 5).await;
        assert!(result.is_ok(), "搜索失败: {:?}", result);

        let response = result.unwrap();
        assert!(!response.organic.is_empty(), "无搜索结果");

        // 验证返回结构
        let first = &response.organic[0];
        assert!(!first.title.is_empty());
        assert!(!first.link.is_empty());
        assert!(!first.snippet.is_empty());
    }

    #[tokio::test]
    #[ignore] // 需要 GLM_API_KEY 环境变量
    async fn test_fetch_integration() {
        let api_key = std::env::var("GLM_API_KEY").unwrap_or_default();
        if api_key.is_empty() {
            eprintln!("跳过: GLM_API_KEY 未设置");
            return;
        }

        let provider = ZhiPuProvider::new(
            "https://open.bigmodel.cn".to_string(),
            api_key,
        );

        let result = provider.fetch("https://www.rust-lang.org/zh-CN/").await;
        assert!(result.is_ok(), "获取失败: {:?}", result);

        let response = result.unwrap();
        assert!(!response.content.is_empty(), "无内容");
    }

    #[tokio::test]
    #[ignore] // 需要 GLM_CODING_API_KEY 环境变量
    async fn test_search_coding_integration() {
        let api_key = std::env::var("GLM_CODING_API_KEY").unwrap_or_default();
        if api_key.is_empty() {
            eprintln!("跳过: GLM_CODING_API_KEY 未设置");
            return;
        }

        let provider = ZhiPuProvider::with_variant(
            "https://open.bigmodel.cn".to_string(),
            api_key,
            "coding",
        );

        let result = provider.search("Rust 编程语言", 5).await;
        assert!(result.is_ok(), "搜索失败: {:?}", result);

        let response = result.unwrap();
        eprintln!("coding搜索返回 {} 条结果", response.organic.len());
        for (i, r) in response.organic.iter().enumerate() {
            eprintln!("[{}] title={}, link={}", i, r.title, r.link);
        }
        assert!(!response.organic.is_empty(), "无搜索结果");

        // 验证返回结构
        let first = &response.organic[0];
        assert!(!first.title.is_empty());
        assert!(!first.link.is_empty());
        assert!(!first.snippet.is_empty());
    }

    #[tokio::test]
    #[ignore] // 需要 GLM_CODING_API_KEY 环境变量
    async fn test_fetch_coding_integration() {
        let api_key = std::env::var("GLM_CODING_API_KEY").unwrap_or_default();
        if api_key.is_empty() {
            eprintln!("跳过: GLM_CODING_API_KEY 未设置");
            return;
        }

        let provider = ZhiPuProvider::with_variant(
            "https://open.bigmodel.cn".to_string(),
            api_key,
            "coding",
        );

        let result = provider.fetch("https://www.rust-lang.org/zh-CN/").await;
        assert!(result.is_ok(), "获取失败: {:?}", result);

        let response = result.unwrap();
        assert!(!response.content.is_empty(), "无内容");
    }
}