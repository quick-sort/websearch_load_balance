//! medRxiv paper search provider.

use crate::error::WebSearchError;
use crate::paper_providers::trait_def::{
    PaperId, PaperResult, PaperSearchProvider, PaperSearchResponse,
};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;

const BASE_URL: &str = "https://api.biorxiv.org/details/medrxiv";

pub struct MedrxivProvider {
    client: Client,
}

impl MedrxivProvider {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct MedrxivResponse {
    #[serde(default)]
    collection: Vec<MedrxivItem>,
}

#[derive(Debug, Deserialize)]
struct MedrxivItem {
    doi: String,
    title: String,
    authors: String,
    #[serde(default)]
    r#abstract: String,
    date: String,
    #[serde(default)]
    category: String,
    #[serde(default)]
    version: String,
}

#[async_trait]
impl PaperSearchProvider for MedrxivProvider {
    fn name(&self) -> &str {
        "medrxiv"
    }

    async fn search(
        &self,
        query: &str,
        max_results: u32,
    ) -> Result<PaperSearchResponse, WebSearchError> {
        let end = chrono_today();
        let start = chrono_days_ago(30);
        let category = query.to_lowercase().replace(' ', "_");

        let url = format!("{}/{}/{}/0", BASE_URL, start, end);
        let resp = self
            .client
            .get(&url)
            .query(&[("category", category.as_str())])
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(WebSearchError::ProviderError(
                resp.status().as_u16() as i32,
                "medRxiv API error".into(),
            ));
        }

        let data: MedrxivResponse = resp.json().await?;
        let papers: Vec<PaperResult> = data
            .collection
            .into_iter()
            .take(max_results as usize)
            .map(|item| {
                let version = if item.version.is_empty() {
                    "1".to_string()
                } else {
                    item.version
                };
                PaperResult {
                    id: PaperId {
                        doi: Some(item.doi.clone()),
                        url: Some(format!(
                            "https://www.medrxiv.org/content/{}v{}",
                            item.doi, version
                        )),
                        ..Default::default()
                    },
                    title: item.title,
                    authors: item.authors.split("; ").map(|s| s.to_string()).collect(),
                    r#abstract: item.r#abstract,
                    pdf_url: Some(format!(
                        "https://www.medrxiv.org/content/{}v{}.full.pdf",
                        item.doi, version
                    )),
                    published_date: Some(item.date),
                    source: "medrxiv".into(),
                    categories: if item.category.is_empty() {
                        vec![]
                    } else {
                        vec![item.category]
                    },
                    citations: None,
                }
            })
            .collect();

        Ok(PaperSearchResponse { papers })
    }
}

// Reuse date helpers from biorxiv
fn chrono_today() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    format_date(now)
}

fn chrono_days_ago(days: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    format_date(now - days * 86400)
}

fn format_date(epoch_secs: u64) -> String {
    let days = epoch_secs / 86400;
    let (y, m, d) = epoch_days_to_ymd(days);
    format!("{:04}-{:02}-{:02}", y, m, d)
}

fn epoch_days_to_ymd(days: u64) -> (u64, u64, u64) {
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_name() {
        let p = MedrxivProvider::new();
        assert_eq!(p.name(), "medrxiv");
        assert!(p.supports_search());
        assert!(!p.supports_fetch());
    }

    #[test]
    fn test_epoch_days_to_ymd() {
        let (y, m, d) = epoch_days_to_ymd(19723);
        assert_eq!((y, m, d), (2024, 1, 1));
    }

    #[test]
    fn test_format_date() {
        let s = format_date(1704067200);
        assert_eq!(s, "2024-01-01");
    }

    #[tokio::test]
    #[ignore] // hits real medRxiv API
    async fn test_search_integration() {
        let p = MedrxivProvider::new();
        let result = p.search("epidemiology", 3).await;
        assert!(result.is_ok(), "search failed: {:?}", result);
        let resp = result.unwrap();
        for paper in &resp.papers {
            assert_eq!(paper.source, "medrxiv");
            assert!(paper.pdf_url.is_some());
        }
    }
}
