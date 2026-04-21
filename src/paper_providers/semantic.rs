//! Semantic Scholar paper search provider.

use crate::error::WebSearchError;
use crate::paper_providers::trait_def::{
    PaperFetchResponse, PaperId, PaperResult, PaperSearchProvider, PaperSearchResponse,
};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;

const BASE_URL: &str = "https://api.semanticscholar.org/graph/v1";

pub struct SemanticProvider {
    client: Client,
    api_key: Option<String>,
}

impl SemanticProvider {
    pub fn new(api_key: Option<String>) -> Self {
        Self {
            client: Client::new(),
            api_key,
        }
    }

    fn add_auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let Some(key) = &self.api_key {
            req.header("x-api-key", key)
        } else {
            req
        }
    }
}

#[derive(Debug, Deserialize)]
struct SemanticSearchResponse {
    #[serde(default)]
    data: Vec<SemanticPaper>,
}

#[derive(Debug, Deserialize)]
struct SemanticPaper {
    #[serde(rename = "paperId")]
    paper_id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    r#abstract: Option<String>,
    #[serde(default)]
    authors: Vec<SemanticAuthor>,
    #[serde(default)]
    url: Option<String>,
    #[serde(rename = "publicationDate", default)]
    publication_date: Option<String>,
    #[serde(rename = "externalIds", default)]
    external_ids: Option<SemanticExternalIds>,
    #[serde(rename = "fieldsOfStudy", default)]
    fields_of_study: Option<Vec<String>>,
    #[serde(rename = "citationCount", default)]
    citation_count: Option<u64>,
    #[serde(rename = "openAccessPdf", default)]
    open_access_pdf: Option<SemanticPdf>,
}

#[derive(Debug, Deserialize)]
struct SemanticAuthor {
    #[serde(default)]
    name: String,
}

#[derive(Debug, Deserialize)]
struct SemanticExternalIds {
    #[serde(rename = "DOI", default)]
    doi: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SemanticPdf {
    #[serde(default)]
    url: Option<String>,
}

const FIELDS: &str = "title,abstract,authors,url,publicationDate,externalIds,fieldsOfStudy,citationCount,openAccessPdf";

impl SemanticPaper {
    fn into_paper_result(self) -> PaperResult {
        let doi = self.external_ids.as_ref().and_then(|e| e.doi.clone());
        PaperResult {
            id: PaperId {
                doi,
                semantic_id: Some(self.paper_id),
                url: self.url.clone(),
                ..Default::default()
            },
            title: self.title,
            authors: self.authors.into_iter().map(|a| a.name).collect(),
            r#abstract: self.r#abstract.unwrap_or_default(),
            pdf_url: self.open_access_pdf.and_then(|p| p.url),
            published_date: self.publication_date,
            source: "semantic".into(),
            categories: self.fields_of_study.unwrap_or_default(),
            citations: self.citation_count,
        }
    }
}

#[async_trait]
impl PaperSearchProvider for SemanticProvider {
    fn name(&self) -> &str {
        "semantic"
    }

    fn supports_fetch(&self) -> bool {
        true
    }

    async fn search(
        &self,
        query: &str,
        max_results: u32,
    ) -> Result<PaperSearchResponse, WebSearchError> {
        let url = format!("{}/paper/search", BASE_URL);
        let req = self.client.get(&url).query(&[
            ("query", query),
            ("limit", &max_results.to_string()),
            ("fields", FIELDS),
        ]);
        let resp = self.add_auth(req).send().await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16() as i32;
            let body = resp.text().await.unwrap_or_default();
            return Err(WebSearchError::ProviderError(status, body));
        }

        let data: SemanticSearchResponse = resp.json().await?;
        let papers = data
            .data
            .into_iter()
            .map(|p| p.into_paper_result())
            .collect();

        Ok(PaperSearchResponse { papers })
    }

    async fn fetch(&self, id: &PaperId) -> Result<PaperFetchResponse, WebSearchError> {
        // Semantic Scholar accepts: raw ID, DOI:xxx, ARXIV:xxx, PMID:xxx, PMCID:xxx, URL:xxx
        let identifier = if let Some(v) = &id.semantic_id {
            v.clone()
        } else if let Some(v) = &id.doi {
            format!("DOI:{}", v)
        } else if let Some(v) = &id.arxiv_id {
            format!("ARXIV:{}", v)
        } else if let Some(v) = &id.pmid {
            format!("PMID:{}", v)
        } else if let Some(v) = &id.pmcid {
            format!("PMCID:{}", v)
        } else if let Some(v) = &id.url {
            format!("URL:{}", v)
        } else {
            return Err(WebSearchError::NotSupported(
                "semantic requires at least one ID".into(),
            ));
        };

        let url = format!("{}/paper/{}", BASE_URL, identifier);
        let req = self.client.get(&url).query(&[("fields", FIELDS)]);
        let resp = self.add_auth(req).send().await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16() as i32;
            let body = resp.text().await.unwrap_or_default();
            return Err(WebSearchError::ProviderError(status, body));
        }

        let paper: SemanticPaper = resp.json().await?;
        let result = paper.into_paper_result();

        Ok(PaperFetchResponse {
            paper_id: result.id.best_id().to_string(),
            title: result.title.clone(),
            content: format!(
                "Title: {}\nAuthors: {}\nAbstract: {}\nDOI: {}\nCitations: {}",
                result.title,
                result.authors.join(", "),
                result.r#abstract,
                result.id.doi.as_deref().unwrap_or("N/A"),
                result.citations.unwrap_or(0),
            ),
            url: result.id.url.unwrap_or_default(),
            source: "semantic".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_name() {
        let p = SemanticProvider::new(None);
        assert_eq!(p.name(), "semantic");
        assert!(p.supports_fetch());
        assert!(p.supports_search());
    }

    #[test]
    fn test_semantic_paper_into_paper_result() {
        let sp = SemanticPaper {
            paper_id: "abc123".into(),
            title: "Test Paper".into(),
            r#abstract: Some("An abstract".into()),
            authors: vec![
                SemanticAuthor {
                    name: "Alice".into(),
                },
                SemanticAuthor { name: "Bob".into() },
            ],
            url: Some("https://example.com".into()),
            publication_date: Some("2023-01-15".into()),
            external_ids: Some(SemanticExternalIds {
                doi: Some("10.1234/test".into()),
            }),
            fields_of_study: Some(vec!["Computer Science".into()]),
            citation_count: Some(42),
            open_access_pdf: Some(SemanticPdf {
                url: Some("https://example.com/paper.pdf".into()),
            }),
        };
        let r = sp.into_paper_result();
        assert_eq!(r.id.semantic_id.as_deref(), Some("abc123"));
        assert_eq!(r.title, "Test Paper");
        assert_eq!(r.authors, vec!["Alice", "Bob"]);
        assert_eq!(r.r#abstract, "An abstract");
        assert_eq!(r.id.doi.as_deref(), Some("10.1234/test"));
        assert_eq!(r.citations, Some(42));
        assert_eq!(r.pdf_url.as_deref(), Some("https://example.com/paper.pdf"));
        assert_eq!(r.source, "semantic");
        assert_eq!(r.categories, vec!["Computer Science"]);
    }

    #[test]
    fn test_semantic_paper_into_paper_result_minimal() {
        let sp = SemanticPaper {
            paper_id: "xyz".into(),
            title: "Minimal".into(),
            r#abstract: None,
            authors: vec![],
            url: None,
            publication_date: None,
            external_ids: None,
            fields_of_study: None,
            citation_count: None,
            open_access_pdf: None,
        };
        let r = sp.into_paper_result();
        assert_eq!(r.id.semantic_id.as_deref(), Some("xyz"));
        assert!(r.r#abstract.is_empty());
        assert!(r.authors.is_empty());
        assert!(r.id.doi.is_none());
        assert!(r.pdf_url.is_none());
    }

    #[tokio::test]
    #[ignore] // hits real Semantic Scholar API
    async fn test_search_integration() {
        let p = SemanticProvider::new(None);
        let result = p.search("attention is all you need", 3).await;
        assert!(result.is_ok(), "search failed: {:?}", result);
        let resp = result.unwrap();
        assert!(!resp.papers.is_empty());
        assert_eq!(resp.papers[0].source, "semantic");
    }

    #[tokio::test]
    #[ignore] // hits real Semantic Scholar API
    async fn test_fetch_integration() {
        let p = SemanticProvider::new(None);
        // Well-known paper: "Attention Is All You Need"
        let result = p
            .fetch(&PaperId {
                semantic_id: Some("204e3073870fae3d05bcbc2f6a8e263d9b72e776".into()),
                ..Default::default()
            })
            .await;
        assert!(result.is_ok(), "fetch failed: {:?}", result);
        let resp = result.unwrap();
        assert!(resp.title.to_lowercase().contains("attention"));
        assert_eq!(resp.source, "semantic");
    }
}
