//! Load balancer manager for providers and API keys.

use crate::config::Config;
use crate::error::WebSearchError;
use crate::load_balancer::strategy::{create_strategy, SelectionStrategy};
use crate::providers::anycrawl::AnycrawlProvider;
use crate::providers::bocha::BochaProvider;
use crate::providers::firecrawl::FirecrawlProvider;
use crate::providers::minimax::MiniMaxProvider;
use crate::providers::serpapi::SerpApiProvider;
use crate::providers::serper::SerperProvider;
use crate::providers::tavily::TavilyProvider;
use crate::providers::trait_def::{FetchResponse, SearchResponse, WebSearchProvider};
use crate::providers::zhipu::ZhiPuProvider;
use std::sync::Arc;
use tracing;

/// A provider instance with an associated key index.
#[derive(Clone)]
struct ProviderEntry {
    provider: Arc<dyn WebSearchProvider>,
    provider_name: String,
    key_index: usize,
    supports_fetch: bool,
    supports_search: bool,
}

/// Provider load balancer.
///
/// Manages multiple providers (each with potentially multiple API keys)
/// and rotates between them according to the configured strategy.
#[derive(Clone)]
pub struct ProviderLoadBalancer {
    /// All provider entries (provider + key combinations), in config priority order.
    entries: Vec<ProviderEntry>,
    /// Strategy for selecting between providers.
    provider_strategy: Arc<dyn SelectionStrategy>,
    /// Whether to fallback to next provider on failure.
    fallback: bool,
}

/// Get the default base URL for a known provider.
fn default_base_url(provider_name: &str) -> &'static str {
    match provider_name {
        "tavily" => "https://api.tavily.com",
        "minimaxi" => "https://api.minimaxi.com",
        "minimax_io" => "https://api.minimaxi.io",
        "zhipu" | "zhipu_coding" => "https://open.bigmodel.cn",
        "bocha" => "https://api.bocha.cn",
        "firecrawl" => "https://api.firecrawl.dev",
        "anycrawl" => "https://api.anycrawl.dev",
        "serpapi" => "https://serpapi.com",
        "serper" => "https://google.serper.dev",
        "webcrawler" => "https://api.webcrawlerapi.com",
        _ => "",
    }
}

impl ProviderLoadBalancer {
    /// Create a new load balancer from configuration.
    pub fn from_config(config: &Config) -> Result<Self, WebSearchError> {
        let mut entries = Vec::new();

        for provider_config in &config.providers {
            if !provider_config.enabled {
                tracing::info!("Provider '{}' is disabled, skipping", provider_config.name);
                continue;
            }

            let base_url: &str = if provider_config.base_url.is_empty() {
                default_base_url(&provider_config.name)
            } else {
                &provider_config.base_url
            };

            if base_url.is_empty() {
                tracing::warn!(
                    "Provider '{}' has no base_url configured and no known default",
                    provider_config.name
                );
                continue;
            }

            let name = provider_config.name.as_str();
            let provider_instances: Vec<Arc<dyn WebSearchProvider>> = match name {
                "tavily" => provider_config
                    .api_keys
                    .iter()
                    .map(|key| {
                        Arc::new(TavilyProvider::new(base_url.to_string(), key.clone()))
                            as Arc<dyn WebSearchProvider>
                    })
                    .collect(),
                "minimaxi" | "minimax_io" => provider_config
                    .api_keys
                    .iter()
                    .map(|key| {
                        Arc::new(MiniMaxProvider::new(base_url.to_string(), key.clone()))
                            as Arc<dyn WebSearchProvider>
                    })
                    .collect(),
                "zhipu" | "zhipu_coding" => {
                    let api_variant = provider_config.settings.variant();
                    provider_config
                        .api_keys
                        .iter()
                        .map(|key| {
                            Arc::new(ZhiPuProvider::with_variant(
                                base_url.to_string(),
                                key.clone(),
                                api_variant,
                            )) as Arc<dyn WebSearchProvider>
                        })
                        .collect()
                }
                "bocha" => provider_config
                    .api_keys
                    .iter()
                    .map(|key| {
                        Arc::new(BochaProvider::new(base_url.to_string(), key.clone()))
                            as Arc<dyn WebSearchProvider>
                    })
                    .collect(),
                "firecrawl" => provider_config
                    .api_keys
                    .iter()
                    .map(|key| {
                        Arc::new(FirecrawlProvider::new(base_url.to_string(), key.clone()))
                            as Arc<dyn WebSearchProvider>
                    })
                    .collect(),
                "anycrawl" => provider_config
                    .api_keys
                    .iter()
                    .map(|key| {
                        Arc::new(AnycrawlProvider::new(base_url.to_string(), key.clone()))
                            as Arc<dyn WebSearchProvider>
                    })
                    .collect(),
                "serpapi" => provider_config
                    .api_keys
                    .iter()
                    .map(|key| {
                        Arc::new(SerpApiProvider::new(base_url.to_string(), key.clone()))
                            as Arc<dyn WebSearchProvider>
                    })
                    .collect(),
                "serper" => provider_config
                    .api_keys
                    .iter()
                    .map(|key| {
                        Arc::new(SerperProvider::new(base_url.to_string(), key.clone()))
                            as Arc<dyn WebSearchProvider>
                    })
                    .collect(),
                _ => {
                    tracing::warn!("Unknown provider: {}", provider_config.name);
                    continue;
                }
            };

            let supports_fetch = !["minimaxi", "minimax_io", "bocha", "serpapi"]
                .contains(&name);
            let supports_search = name != "anycrawl";

            for (key_index, provider) in provider_instances.into_iter().enumerate() {
                entries.push(ProviderEntry {
                    provider_name: provider_config.name.clone(),
                    key_index,
                    supports_fetch,
                    supports_search,
                    provider,
                });
            }

            tracing::info!(
                "Loaded provider '{}' with {} API key(s)",
                provider_config.name,
                provider_config.api_keys.len()
            );
        }

        if entries.is_empty() {
            return Err(WebSearchError::NoProvidersAvailable);
        }

        let provider_strategy: Arc<dyn SelectionStrategy> =
            create_strategy(config.provider_strategy.r#type).into();

        tracing::info!(
            "Load balancer initialized with {} provider+key entries",
            entries.len()
        );

        Ok(Self {
            entries,
            provider_strategy,
            fallback: config.provider_strategy.fallback,
        })
    }

    /// Search using the configured providers with load balancing.
    pub async fn search(
        &self,
        query: &str,
        max_results: u32,
    ) -> Result<SearchResponse, WebSearchError> {
        // Filter to entries that support search
        let search_indices: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.supports_search)
            .map(|(i, _)| i)
            .collect();

        if search_indices.is_empty() {
            return Err(WebSearchError::NoProvidersAvailable);
        }

        if self.fallback {
            let start = self.provider_strategy.select_index(search_indices.len());
            let mut errors = Vec::new();

            for offset in 0..search_indices.len() {
                let idx = (start + offset) % search_indices.len();
                let entry = &self.entries[search_indices[idx]];

                tracing::info!(
                    "Trying provider '{}' (key #{})",
                    entry.provider_name,
                    entry.key_index
                );

                match entry.provider.search(query, max_results).await {
                    Ok(response) => {
                        tracing::info!(
                            "Search succeeded with provider '{}' (key #{})",
                            entry.provider_name,
                            entry.key_index
                        );
                        return Ok(response);
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Provider '{}' (key #{}) failed: {}",
                            entry.provider_name,
                            entry.key_index,
                            e
                        );
                        errors.push((entry.provider_name.clone(), entry.key_index, e));
                    }
                }
            }

            Err(WebSearchError::AllProvidersFailed(format!(
                "All providers failed: {}",
                errors
                    .iter()
                    .map(|(name, idx, e)| format!("{}[{}]={}", name, idx, e))
                    .collect::<Vec<_>>()
                    .join(", ")
            )))
        } else {
            let index = self.provider_strategy.select_index(search_indices.len());
            let entry = &self.entries[search_indices[index]];
            entry.provider.search(query, max_results).await
        }
    }

    /// Fetch URL content using the configured providers with load balancing.
    ///
    /// Only providers that support fetch will be used.
    pub async fn fetch(&self, url: &str) -> Result<FetchResponse, WebSearchError> {
        let fetch_indices: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.supports_fetch)
            .map(|(i, _)| i)
            .collect();

        if fetch_indices.is_empty() {
            return Err(WebSearchError::AllProvidersFailed(
                "No providers support web fetch".to_string(),
            ));
        }

        if self.fallback {
            let start = self.provider_strategy.select_index(fetch_indices.len());
            let mut errors = Vec::new();

            for offset in 0..fetch_indices.len() {
                let idx = (start + offset) % fetch_indices.len();
                let entry = &self.entries[fetch_indices[idx]];

                tracing::info!(
                    "Trying fetch with provider '{}' (key #{})",
                    entry.provider_name,
                    entry.key_index
                );

                match entry.provider.fetch(url).await {
                    Ok(response) => return Ok(response),
                    Err(e) => {
                        tracing::warn!("Provider '{}' fetch failed: {}", entry.provider_name, e);
                        errors.push((entry.provider_name.clone(), entry.key_index, e));
                    }
                }
            }

            Err(WebSearchError::AllProvidersFailed(format!(
                "All providers failed to fetch: {}",
                errors
                    .iter()
                    .map(|(name, idx, e)| format!("{}[{}]={}", name, idx, e))
                    .collect::<Vec<_>>()
                    .join(", ")
            )))
        } else {
            let index = self.provider_strategy.select_index(fetch_indices.len());
            let entry = &self.entries[fetch_indices[index]];
            entry.provider.fetch(url).await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{LoadBalanceStrategy, ProviderConfig};

    fn sample_config() -> Config {
        Config {
            server: Default::default(),
            key_strategy: Default::default(),
            provider_strategy: LoadBalanceStrategy {
                r#type: crate::config::StrategyType::RoundRobin,
                fallback: true,
            },
            providers: vec![
                ProviderConfig {
                    name: "tavily".to_string(),
                    enabled: true,
                    base_url: "https://api.tavily.com".to_string(),
                    api_keys: vec!["key1".to_string(), "key2".to_string()],
                    settings: Default::default(),
                },
                ProviderConfig {
                    name: "zhipu".to_string(),
                    enabled: true,
                    base_url: "https://open.bigmodel.cn".to_string(),
                    api_keys: vec!["key1".to_string()],
                    settings: Default::default(),
                },
                ProviderConfig {
                    name: "minimaxi".to_string(),
                    enabled: true,
                    base_url: "https://api.minimaxi.com".to_string(),
                    api_keys: vec!["key1".to_string()],
                    settings: Default::default(),
                },
                ProviderConfig {
                    name: "disabled_provider".to_string(),
                    enabled: false,
                    base_url: "https://example.com".to_string(),
                    api_keys: vec!["key1".to_string()],
                    settings: Default::default(),
                },
            ],
        }
    }

    #[test]
    fn test_from_config() {
        let config = sample_config();
        let lb = ProviderLoadBalancer::from_config(&config).unwrap();
        // 2 tavily keys + 1 zhipu key + 1 minimax key = 4 entries
        assert_eq!(lb.entries.len(), 4);
        // Check first two are tavily
        assert_eq!(lb.entries[0].provider_name, "tavily");
        assert_eq!(lb.entries[1].provider_name, "tavily");
        assert_eq!(lb.entries[2].provider_name, "zhipu");
        assert_eq!(lb.entries[3].provider_name, "minimaxi");
    }

    #[test]
    fn test_no_providers() {
        let config = Config::default(); // No providers
        let result = ProviderLoadBalancer::from_config(&config);
        match result {
            Err(WebSearchError::NoProvidersAvailable) => {}
            Err(other) => panic!("Expected NoProvidersAvailable, got: {}", other),
            Ok(_) => panic!("Expected error but got success"),
        }
    }

    fn integration_config(keys: &[(&str, &str)]) -> Option<Config> {
        let providers: Vec<ProviderConfig> = keys
            .iter()
            .filter_map(|&(name, key)| {
                let key_value = crate::error::parse_api_key(key);
                if key_value.is_empty() {
                    return None;
                }
                let (base_url, provider_name) = match name {
                    "tavily" => ("https://api.tavily.com".to_string(), "tavily".to_string()),
                    "minimaxi" => (
                        "https://api.minimaxi.com".to_string(),
                        "minimaxi".to_string(),
                    ),
                    "zhipu" => ("https://open.bigmodel.cn".to_string(), "zhipu".to_string()),
                    "bocha" => ("https://api.bocha.cn".to_string(), "bocha".to_string()),
                    "firecrawl" => (
                        "https://api.firecrawl.dev".to_string(),
                        "firecrawl".to_string(),
                    ),
                    "anycrawl" => (
                        "https://api.anycrawl.dev".to_string(),
                        "anycrawl".to_string(),
                    ),
                    "serpapi" => ("https://serpapi.com".to_string(), "serpapi".to_string()),
                    "serper" => (
                        "https://google.serper.dev".to_string(),
                        "serper".to_string(),
                    ),
                    _ => return None,
                };
                Some(ProviderConfig {
                    name: provider_name,
                    enabled: true,
                    base_url,
                    api_keys: vec![key_value],
                    settings: Default::default(),
                })
            })
            .collect();

        if providers.is_empty() {
            return None;
        }

        Some(Config {
            server: Default::default(),
            key_strategy: Default::default(),
            provider_strategy: LoadBalanceStrategy {
                r#type: crate::config::StrategyType::RoundRobin,
                fallback: true,
            },
            providers,
        })
    }

    #[tokio::test]
    #[ignore] // 需要 TAVILY_API_KEYS 和/或 GLM_API_KEYS
    async fn test_search_integration() {
        let config =
            match integration_config(&[("tavily", "TAVILY_API_KEYS"), ("zhipu", "GLM_API_KEYS")]) {
                Some(c) => c,
                None => {
                    eprintln!("跳过: 未设置 TAVILY_API_KEYS 或 GLM_API_KEYS");
                    return;
                }
            };
        let lb = ProviderLoadBalancer::from_config(&config).unwrap();

        let result = lb.search("Rust programming language", 5).await;
        assert!(result.is_ok(), "搜索失败: {:?}", result);

        let response = result.unwrap();
        assert!(!response.organic.is_empty(), "无搜索结果");
        assert!(!response.organic[0].title.is_empty());
    }

    #[tokio::test]
    #[ignore] // 需要 TAVILY_API_KEYS 和/或 GLM_API_KEYS
    async fn test_fetch_integration() {
        let config =
            match integration_config(&[("tavily", "TAVILY_API_KEYS"), ("zhipu", "GLM_API_KEYS")]) {
                Some(c) => c,
                None => {
                    eprintln!("跳过: 未设置 TAVILY_API_KEYS 或 GLM_API_KEYS");
                    return;
                }
            };
        let lb = ProviderLoadBalancer::from_config(&config).unwrap();

        let result = lb.fetch("https://www.rust-lang.org/").await;
        assert!(result.is_ok(), "获取失败: {:?}", result);

        let response = result.unwrap();
        assert!(!response.content.is_empty(), "无内容");
    }

    #[tokio::test]
    #[ignore] // 需要 GLM_API_KEYS（fallback 场景）
    async fn test_fallback_integration() {
        let mut config = match integration_config(&[("zhipu", "GLM_API_KEYS")]) {
            Some(c) => c,
            None => {
                eprintln!("跳过: 未设置 GLM_API_KEYS");
                return;
            }
        };
        // 在最前面插入一个无效 provider，测试 fallback
        config.providers.insert(
            0,
            ProviderConfig {
                name: "tavily_invalid".to_string(),
                enabled: true,
                base_url: "https://api.tavily.com".to_string(),
                api_keys: vec!["tvly-invalid-key-for-testing".to_string()],
                settings: Default::default(),
            },
        );

        let lb = ProviderLoadBalancer::from_config(&config).unwrap();
        // 虽然 tavily 会失败，但 fallback 到 zhipu 应该成功
        let result = lb.search("Rust language", 3).await;
        assert!(result.is_ok(), "fallback 搜索失败: {:?}", result);

        let response = result.unwrap();
        assert!(!response.organic.is_empty());
    }
}
