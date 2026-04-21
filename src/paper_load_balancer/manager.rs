//! Load balancer for paper search providers.

use crate::config::Config;
use crate::error::WebSearchError;
use crate::load_balancer::strategy::{create_strategy, SelectionStrategy};
use crate::paper_providers::arxiv::ArxivProvider;
use crate::paper_providers::biorxiv::BiorxivProvider;
use crate::paper_providers::google_scholar::GoogleScholarProvider;
use crate::paper_providers::medrxiv::MedrxivProvider;
use crate::paper_providers::pmc::PmcProvider;
use crate::paper_providers::pubmed::PubmedProvider;
use crate::paper_providers::sci_hub::SciHubProvider;
use crate::paper_providers::semantic::SemanticProvider;
use crate::paper_providers::trait_def::{
    PaperFetchResponse, PaperId, PaperSearchProvider, PaperSearchResponse,
};
use std::collections::HashMap;
use std::sync::Arc;
use tracing;

/// A provider entry with key index.
#[derive(Clone)]
struct PaperProviderEntry {
    provider: Arc<dyn PaperSearchProvider>,
    source_name: String,
    key_index: usize,
}

/// Paper provider load balancer.
///
/// Manages paper providers keyed by source name.
/// Each source can have multiple key entries for rotation.
#[derive(Clone)]
pub struct PaperLoadBalancer {
    /// Entries grouped by source name.
    entries_by_source: HashMap<String, Vec<PaperProviderEntry>>,
    /// Strategy for key selection within a source.
    key_strategy: Arc<dyn SelectionStrategy>,
    /// Whether to fallback to next source on failure.
    fallback: bool,
}

impl PaperLoadBalancer {
    /// Create from config. If no paper_providers configured, creates defaults (no-key providers).
    pub fn from_config(config: &Config) -> Self {
        let mut entries_by_source: HashMap<String, Vec<PaperProviderEntry>> = HashMap::new();

        // Check if config has paper_providers
        let has_paper_config = !config.paper_providers.is_empty();

        if has_paper_config {
            for pc in &config.paper_providers {
                if !pc.enabled {
                    tracing::info!("Paper provider '{}' is disabled, skipping", pc.name);
                    continue;
                }
                let name = pc.name.as_str();
                let providers = create_paper_providers(name, &pc.api_keys, &pc.base_url);
                for (key_index, provider) in providers.into_iter().enumerate() {
                    entries_by_source.entry(name.to_string()).or_default().push(
                        PaperProviderEntry {
                            provider,
                            source_name: name.to_string(),
                            key_index,
                        },
                    );
                }
                tracing::info!(
                    "Loaded paper provider '{}' with {} key(s)",
                    pc.name,
                    pc.api_keys.len().max(1)
                );
            }
        } else {
            // Register all default providers (no keys)
            for name in &[
                "google_scholar",
                "pubmed",
                "pmc",
                "arxiv",
                "biorxiv",
                "medrxiv",
                "semantic",
                "sci_hub",
            ] {
                let providers = create_paper_providers(name, &[], &String::new());
                for (key_index, provider) in providers.into_iter().enumerate() {
                    entries_by_source.entry(name.to_string()).or_default().push(
                        PaperProviderEntry {
                            provider,
                            source_name: name.to_string(),
                            key_index,
                        },
                    );
                }
            }
            tracing::info!("No paper_providers configured, using defaults (no API keys)");
        }

        let key_strategy: Arc<dyn SelectionStrategy> =
            create_strategy(config.key_strategy.r#type).into();

        Self {
            entries_by_source,
            key_strategy,
            fallback: config.provider_strategy.fallback,
        }
    }

    /// Search papers using the specified sources.
    /// Results from all requested sources are merged.
    pub async fn search(
        &self,
        query: &str,
        max_results: u32,
        sources: &[String],
    ) -> Result<PaperSearchResponse, WebSearchError> {
        let mut all_papers = Vec::new();
        let mut errors = Vec::new();
        let per_source = (max_results as usize / sources.len().max(1)).max(1) as u32;

        for source in sources {
            let entries = match self.entries_by_source.get(source.as_str()) {
                Some(e) if !e.is_empty() => e,
                _ => {
                    tracing::warn!("Unknown or unconfigured paper source: {}", source);
                    errors.push(format!("{}: not configured", source));
                    continue;
                }
            };

            // Filter to entries that support search
            let search_entries: Vec<&PaperProviderEntry> = entries
                .iter()
                .filter(|e| e.provider.supports_search())
                .collect();
            if search_entries.is_empty() {
                errors.push(format!("{}: does not support search", source));
                continue;
            }

            let start = self.key_strategy.select_index(search_entries.len());
            let mut succeeded = false;

            for offset in 0..search_entries.len() {
                let idx = (start + offset) % search_entries.len();
                let entry = search_entries[idx];

                tracing::info!(
                    "Trying paper source '{}' (key #{})",
                    entry.source_name,
                    entry.key_index
                );

                match entry.provider.search(query, per_source).await {
                    Ok(resp) => {
                        all_papers.extend(resp.papers);
                        succeeded = true;
                        break;
                    }
                    Err(e) => {
                        tracing::warn!("Paper source '{}' failed: {}", entry.source_name, e);
                        errors.push(format!("{}[{}]={}", entry.source_name, entry.key_index, e));
                        if !self.fallback {
                            break;
                        }
                    }
                }
            }

            if !succeeded {
                tracing::warn!("All keys for source '{}' failed", source);
            }
        }

        if all_papers.is_empty() && !errors.is_empty() {
            return Err(WebSearchError::AllProvidersFailed(errors.join(", ")));
        }

        // Truncate to max_results
        all_papers.truncate(max_results as usize);
        Ok(PaperSearchResponse { papers: all_papers })
    }

    /// Fetch a paper by identifier, trying the specified sources.
    pub async fn fetch(
        &self,
        id: &PaperId,
        sources: &[String],
    ) -> Result<PaperFetchResponse, WebSearchError> {
        let mut errors = Vec::new();

        for source in sources {
            let entries = match self.entries_by_source.get(source.as_str()) {
                Some(e) if !e.is_empty() => e,
                _ => {
                    errors.push(format!("{}: not configured", source));
                    continue;
                }
            };

            let fetch_entries: Vec<&PaperProviderEntry> = entries
                .iter()
                .filter(|e| e.provider.supports_fetch())
                .collect();
            if fetch_entries.is_empty() {
                errors.push(format!("{}: does not support fetch", source));
                continue;
            }

            let start = self.key_strategy.select_index(fetch_entries.len());
            for offset in 0..fetch_entries.len() {
                let idx = (start + offset) % fetch_entries.len();
                let entry = fetch_entries[idx];

                match entry.provider.fetch(id).await {
                    Ok(resp) => return Ok(resp),
                    Err(e) => {
                        errors.push(format!("{}[{}]={}", entry.source_name, entry.key_index, e));
                        if !self.fallback {
                            break;
                        }
                    }
                }
            }
        }

        Err(WebSearchError::AllProvidersFailed(errors.join(", ")))
    }
}

/// Create provider instances for a given source name.
fn create_paper_providers(
    name: &str,
    api_keys: &[String],
    base_url: &str,
) -> Vec<Arc<dyn PaperSearchProvider>> {
    match name {
        "google_scholar" => vec![Arc::new(GoogleScholarProvider::new())],
        "arxiv" => vec![Arc::new(ArxivProvider::new())],
        "biorxiv" => vec![Arc::new(BiorxivProvider::new())],
        "medrxiv" => vec![Arc::new(MedrxivProvider::new())],
        "pmc" => {
            if api_keys.is_empty() {
                vec![Arc::new(PmcProvider::new(None))]
            } else {
                api_keys
                    .iter()
                    .map(|k| {
                        Arc::new(PmcProvider::new(Some(k.clone()))) as Arc<dyn PaperSearchProvider>
                    })
                    .collect()
            }
        }
        "pubmed" => {
            if api_keys.is_empty() {
                vec![Arc::new(PubmedProvider::new(None))]
            } else {
                api_keys
                    .iter()
                    .map(|k| {
                        Arc::new(PubmedProvider::new(Some(k.clone())))
                            as Arc<dyn PaperSearchProvider>
                    })
                    .collect()
            }
        }
        "semantic" => {
            if api_keys.is_empty() {
                vec![Arc::new(SemanticProvider::new(None))]
            } else {
                api_keys
                    .iter()
                    .map(|k| {
                        Arc::new(SemanticProvider::new(Some(k.clone())))
                            as Arc<dyn PaperSearchProvider>
                    })
                    .collect()
            }
        }
        "sci_hub" => {
            let url = if base_url.is_empty() {
                None
            } else {
                Some(base_url.to_string())
            };
            vec![Arc::new(SciHubProvider::new(url))]
        }
        _ => {
            tracing::warn!("Unknown paper provider: {}", name);
            vec![]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, PaperProviderConfig};

    fn default_config() -> Config {
        Config {
            paper_providers: vec![],
            ..Default::default()
        }
    }

    #[test]
    fn test_default_config_registers_all_sources() {
        let lb = PaperLoadBalancer::from_config(&default_config());
        // All 7 default sources should be registered
        for name in &[
            "google_scholar",
            "pubmed",
            "pmc",
            "arxiv",
            "biorxiv",
            "medrxiv",
            "semantic",
            "sci_hub",
        ] {
            assert!(
                lb.entries_by_source.contains_key(*name),
                "missing default source: {}",
                name
            );
        }
    }

    #[test]
    fn test_explicit_config_only_registers_configured() {
        let config = Config {
            paper_providers: vec![
                PaperProviderConfig {
                    name: "arxiv".into(),
                    enabled: true,
                    base_url: String::new(),
                    api_keys: vec![],
                },
                PaperProviderConfig {
                    name: "pubmed".into(),
                    enabled: false,
                    base_url: String::new(),
                    api_keys: vec![],
                },
            ],
            ..Default::default()
        };
        let lb = PaperLoadBalancer::from_config(&config);
        assert!(lb.entries_by_source.contains_key("arxiv"));
        assert!(!lb.entries_by_source.contains_key("pubmed")); // disabled
        assert!(!lb.entries_by_source.contains_key("google_scholar")); // not in config
    }

    #[test]
    fn test_multi_key_creates_multiple_entries() {
        let config = Config {
            paper_providers: vec![PaperProviderConfig {
                name: "semantic".into(),
                enabled: true,
                base_url: String::new(),
                api_keys: vec!["key1".into(), "key2".into(), "key3".into()],
            }],
            ..Default::default()
        };
        let lb = PaperLoadBalancer::from_config(&config);
        assert_eq!(lb.entries_by_source["semantic"].len(), 3);
    }

    #[test]
    fn test_unknown_provider_ignored() {
        let config = Config {
            paper_providers: vec![PaperProviderConfig {
                name: "nonexistent".into(),
                enabled: true,
                base_url: String::new(),
                api_keys: vec![],
            }],
            ..Default::default()
        };
        let lb = PaperLoadBalancer::from_config(&config);
        assert!(!lb.entries_by_source.contains_key("nonexistent"));
    }

    #[tokio::test]
    async fn test_search_unknown_source_errors() {
        let lb = PaperLoadBalancer::from_config(&default_config());
        let result = lb.search("test", 5, &["nonexistent_source".into()]).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_search_sci_hub_not_searchable() {
        let config = Config {
            paper_providers: vec![PaperProviderConfig {
                name: "sci_hub".into(),
                enabled: true,
                base_url: String::new(),
                api_keys: vec![],
            }],
            ..Default::default()
        };
        let lb = PaperLoadBalancer::from_config(&config);
        let result = lb.search("test", 5, &["sci_hub".into()]).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_fetch_google_scholar_not_fetchable() {
        let config = Config {
            paper_providers: vec![PaperProviderConfig {
                name: "google_scholar".into(),
                enabled: true,
                base_url: String::new(),
                api_keys: vec![],
            }],
            ..Default::default()
        };
        let lb = PaperLoadBalancer::from_config(&config);
        let result = lb
            .fetch(
                &PaperId {
                    doi: Some("10.1234/test".into()),
                    ..Default::default()
                },
                &["google_scholar".into()],
            )
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    #[ignore] // hits real arXiv API
    async fn test_search_integration() {
        let lb = PaperLoadBalancer::from_config(&default_config());
        let result = lb.search("machine learning", 3, &["arxiv".into()]).await;
        assert!(result.is_ok(), "search failed: {:?}", result);
        let resp = result.unwrap();
        assert!(!resp.papers.is_empty());
    }

    #[tokio::test]
    #[ignore] // hits real arXiv API
    async fn test_fetch_integration() {
        let lb = PaperLoadBalancer::from_config(&default_config());
        let result = lb
            .fetch(
                &PaperId {
                    arxiv_id: Some("2106.09685".into()),
                    ..Default::default()
                },
                &["arxiv".into()],
            )
            .await;
        assert!(result.is_ok(), "fetch failed: {:?}", result);
        let resp = result.unwrap();
        assert!(
            resp.title.to_lowercase().contains("lora"),
            "unexpected title: {}",
            resp.title
        );
    }

    #[tokio::test]
    #[ignore] // hits real APIs
    async fn test_search_multi_source_integration() {
        let lb = PaperLoadBalancer::from_config(&default_config());
        let result = lb
            .search("CRISPR", 6, &["arxiv".into(), "pubmed".into()])
            .await;
        assert!(result.is_ok(), "multi-source search failed: {:?}", result);
        let resp = result.unwrap();
        assert!(!resp.papers.is_empty());
    }
}
