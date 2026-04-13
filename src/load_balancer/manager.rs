//! Load balancer manager for providers and API keys.

use crate::config::{Config, LoadBalanceStrategy, ProviderConfig};
use crate::error::WebSearchError;
use crate::load_balancer::strategy::{create_strategy, SelectionStrategy};
use crate::providers::minimax::MiniMaxProvider;
use crate::providers::tavily::TavilyProvider;
use crate::providers::trait_def::{FetchResponse, SearchResponse, WebSearchProvider};
use crate::providers::zhipu::ZhiPuProvider;
use std::sync::Arc;
use tracing;

/// A provider instance with an associated key index.
struct ProviderEntry {
    provider: Arc<dyn WebSearchProvider>,
    provider_name: String,
    key_index: usize,
    supports_fetch: bool,
}

/// Provider load balancer.
///
/// Manages multiple providers (each with potentially multiple API keys)
/// and rotates between them according to the configured strategy.
pub struct ProviderLoadBalancer {
    /// All provider entries (provider + key combinations), in config priority order.
    entries: Vec<ProviderEntry>,
    /// Strategy for selecting between providers.
    provider_strategy: Box<dyn SelectionStrategy>,
    /// Whether to fallback to next provider on failure.
    fallback: bool,
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

            let base_url = &provider_config.base_url;
            let provider_instances: Vec<Arc<dyn WebSearchProvider>> =
                match provider_config.name.as_str() {
                    "tavily" => provider_config
                        .api_keys
                        .iter()
                        .enumerate()
                        .map(|(_, key)| {
                            Arc::new(TavilyProvider::new(base_url.clone(), key.clone()))
                                as Arc<dyn WebSearchProvider>
                        })
                        .collect(),
                    "minimax" => provider_config
                        .api_keys
                        .iter()
                        .enumerate()
                        .map(|(_, key)| {
                            Arc::new(MiniMaxProvider::new(base_url.clone(), key.clone()))
                                as Arc<dyn WebSearchProvider>
                        })
                        .collect(),
                    "zhipu" => provider_config
                        .api_keys
                        .iter()
                        .enumerate()
                        .map(|(_, key)| {
                            Arc::new(ZhiPuProvider::new(base_url.clone(), key.clone()))
                                as Arc<dyn WebSearchProvider>
                        })
                        .collect(),
                    _ => {
                        tracing::warn!("Unknown provider: {}", provider_config.name);
                        continue;
                    }
                };

            for (key_index, provider) in provider_instances.into_iter().enumerate() {
                entries.push(ProviderEntry {
                    provider_name: provider_config.name.clone(),
                    key_index,
                    supports_fetch: provider_config.name != "minimax", // MiniMax doesn't support fetch
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

        let provider_strategy = create_strategy(config.provider_strategy.r#type);

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

    /// Select the next provider entry according to the strategy.
    fn select_provider(&self) -> Option<&ProviderEntry> {
        if self.entries.is_empty() {
            return None;
        }
        let index = self.provider_strategy.select_index(self.entries.len());
        self.entries.get(index)
    }

    /// Search using the configured providers with load balancing.
    pub async fn search(&self, query: &str, max_results: u32) -> Result<SearchResponse, WebSearchError> {
        if self.fallback {
            // Try all providers in strategy order
            let mut tried = vec![false; self.entries.len()];
            let mut errors = Vec::new();

            for _ in 0..self.entries.len() {
                let index = self.provider_strategy.select_index(self.entries.len());
                if tried[index] {
                    continue;
                }
                tried[index] = true;

                let entry = &self.entries[index];
                tracing::debug!(
                    "Trying provider '{}' (key #{})",
                    entry.provider_name,
                    entry.key_index
                );

                match entry.provider.search(query, max_results).await {
                    Ok(response) => {
                        tracing::debug!(
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
            // No fallback - just try the selected provider
            let entry = self
                .select_provider()
                .ok_or(WebSearchError::NoProvidersAvailable)?;

            entry.provider.search(query, max_results).await
        }
    }

    /// Fetch URL content using the configured providers with load balancing.
    ///
    /// Only providers that support fetch will be used (skips MiniMax).
    pub async fn fetch(&self, url: &str) -> Result<FetchResponse, WebSearchError> {
        // Get indices of providers that support fetch
        let fetch_entries: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.supports_fetch)
            .map(|(i, _)| i)
            .collect();

        if fetch_entries.is_empty() {
            return Err(WebSearchError::AllProvidersFailed(
                "No providers support web fetch".to_string(),
            ));
        }

        if self.fallback {
            let mut tried = vec![false; fetch_entries.len()];

            for _ in 0..fetch_entries.len() {
                let strategy_index = self.provider_strategy.select_index(fetch_entries.len());
                if tried[strategy_index] {
                    continue;
                }
                tried[strategy_index] = true;

                let entry_index = fetch_entries[strategy_index];
                let entry = &self.entries[entry_index];

                tracing::debug!(
                    "Trying fetch with provider '{}' (key #{})",
                    entry.provider_name,
                    entry.key_index
                );

                match entry.provider.fetch(url).await {
                    Ok(response) => return Ok(response),
                    Err(e) => {
                        tracing::warn!(
                            "Provider '{}' fetch failed: {}",
                            entry.provider_name,
                            e
                        );
                    }
                }
            }

            Err(WebSearchError::AllProvidersFailed(
                "All providers failed to fetch URL".to_string(),
            ))
        } else {
            let index = self.provider_strategy.select_index(fetch_entries.len());
            let entry = &self.entries[fetch_entries[index]];
            entry.provider.fetch(url).await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
                    name: "minimax".to_string(),
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
        assert_eq!(lb.entries[3].provider_name, "minimax");
    }

    #[test]
    fn test_no_providers() {
        let config = Config::default(); // No providers
        let result = ProviderLoadBalancer::from_config(&config);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            WebSearchError::NoProvidersAvailable
        ));
    }
}