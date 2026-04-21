//! Paper provider trait definition for academic search services.

use crate::error::WebSearchError;
use async_trait::async_trait;
use rmcp::schemars;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Structured paper identifier. A paper can have multiple IDs across different systems.
/// Providers pick whichever ID they can use.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct PaperId {
    /// DOI (e.g. "10.1038/nature12373")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doi: Option<String>,
    /// PubMed ID (e.g. "19872477")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pmid: Option<String>,
    /// PubMed Central ID (e.g. "PMC6267067")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pmcid: Option<String>,
    /// arXiv ID (e.g. "2106.09685")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arxiv_id: Option<String>,
    /// Semantic Scholar ID (e.g. "204e3073870fae3d05bcbc2f6a8e263d9b72e776")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub semantic_id: Option<String>,
    /// URL to the paper
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

impl PaperId {
    /// Return the best available identifier as a display string.
    pub fn best_id(&self) -> &str {
        self.doi
            .as_deref()
            .or(self.pmid.as_deref())
            .or(self.pmcid.as_deref())
            .or(self.arxiv_id.as_deref())
            .or(self.semantic_id.as_deref())
            .or(self.url.as_deref())
            .unwrap_or("")
    }

    /// True if at least one ID is set.
    pub fn is_empty(&self) -> bool {
        self.doi.is_none()
            && self.pmid.is_none()
            && self.pmcid.is_none()
            && self.arxiv_id.is_none()
            && self.semantic_id.is_none()
            && self.url.is_none()
    }
}

impl fmt::Display for PaperId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts = Vec::new();
        if let Some(v) = &self.doi {
            parts.push(format!("doi:{}", v));
        }
        if let Some(v) = &self.pmid {
            parts.push(format!("pmid:{}", v));
        }
        if let Some(v) = &self.pmcid {
            parts.push(format!("pmcid:{}", v));
        }
        if let Some(v) = &self.arxiv_id {
            parts.push(format!("arxiv:{}", v));
        }
        if let Some(v) = &self.semantic_id {
            parts.push(format!("s2:{}", v));
        }
        if let Some(v) = &self.url {
            parts.push(format!("url:{}", v));
        }
        write!(f, "PaperId({})", parts.join(", "))
    }
}

/// A single paper result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperResult {
    pub id: PaperId,
    pub title: String,
    pub authors: Vec<String>,
    pub r#abstract: String,
    #[serde(default)]
    pub pdf_url: Option<String>,
    #[serde(default)]
    pub published_date: Option<String>,
    pub source: String,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub citations: Option<u64>,
}

impl PaperResult {
    /// Build a PaperId from this result's known identifiers.
    pub fn to_paper_id(&self) -> &PaperId {
        &self.id
    }
}

/// Response from a paper search.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperSearchResponse {
    pub papers: Vec<PaperResult>,
}

/// Response from a paper fetch (full text / content).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperFetchResponse {
    pub paper_id: String,
    pub title: String,
    pub content: String,
    pub url: String,
    pub source: String,
}

impl fmt::Display for PaperSearchResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PaperSearchResponse(papers: {})", self.papers.len())
    }
}

impl fmt::Display for PaperFetchResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PaperFetchResponse(id: {})", self.paper_id)
    }
}

/// Trait for academic paper search providers.
#[async_trait]
pub trait PaperSearchProvider: Send + Sync {
    /// Provider name.
    fn name(&self) -> &str;

    /// Whether this provider supports search.
    fn supports_search(&self) -> bool {
        true
    }

    /// Whether this provider supports fetch (full text retrieval).
    fn supports_fetch(&self) -> bool {
        false
    }

    /// Search for papers.
    async fn search(
        &self,
        query: &str,
        max_results: u32,
    ) -> Result<PaperSearchResponse, WebSearchError>;

    /// Fetch full content of a paper. Provider picks the ID it can use from PaperId.
    async fn fetch(&self, _id: &PaperId) -> Result<PaperFetchResponse, WebSearchError> {
        Err(WebSearchError::NotSupported(format!(
            "Provider '{}' does not support paper fetch",
            self.name()
        )))
    }
}
