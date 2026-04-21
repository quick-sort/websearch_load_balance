//! PMC (PubMed Central) full text fetch provider via NCBI efetch (JATS XML).
//! Covers all PMC articles, not just the OA subset.

use crate::error::WebSearchError;
use crate::paper_providers::trait_def::{
    PaperFetchResponse, PaperId, PaperSearchProvider, PaperSearchResponse,
};
use async_trait::async_trait;
use reqwest::Client;

const EFETCH_URL: &str = "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/efetch.fcgi";

pub struct PmcProvider {
    client: Client,
    api_key: Option<String>,
}

impl PmcProvider {
    pub fn new(api_key: Option<String>) -> Self {
        Self {
            client: Client::new(),
            api_key,
        }
    }
}

#[async_trait]
impl PaperSearchProvider for PmcProvider {
    fn name(&self) -> &str {
        "pmc"
    }

    fn supports_search(&self) -> bool {
        false
    }

    fn supports_fetch(&self) -> bool {
        true
    }

    async fn search(
        &self,
        _query: &str,
        _max_results: u32,
    ) -> Result<PaperSearchResponse, WebSearchError> {
        Err(WebSearchError::NotSupported(
            "PMC does not support search, use pubmed for search then pmc for full text".into(),
        ))
    }

    async fn fetch(&self, id: &PaperId) -> Result<PaperFetchResponse, WebSearchError> {
        let raw = id
            .pmcid
            .as_deref()
            .ok_or_else(|| WebSearchError::NotSupported("pmc requires pmcid".into()))?;
        let pmcid = normalize_pmcid(raw);

        let mut params = vec![
            ("db", "pmc".to_string()),
            ("id", pmcid.clone()),
            ("retmode", "xml".to_string()),
        ];
        if let Some(key) = &self.api_key {
            params.push(("api_key", key.clone()));
        }

        let resp = self.client.get(EFETCH_URL).query(&params).send().await?;
        if !resp.status().is_success() {
            return Err(WebSearchError::ProviderError(
                resp.status().as_u16() as i32,
                format!("PMC efetch error for {}", pmcid),
            ));
        }

        let xml = resp.text().await?;
        if xml.contains("<error>") || !xml.contains("<article") {
            return Err(WebSearchError::ParseError(format!(
                "Article not found in PMC: {}",
                pmcid
            )));
        }

        let title = extract_tag(&xml, "article-title").unwrap_or_default();
        let abstract_text = extract_body_text(&xml, "abstract");
        let body_text = extract_body_text(&xml, "body");

        let content = if body_text.is_empty() {
            if abstract_text.is_empty() {
                return Err(WebSearchError::ParseError(format!(
                    "No text content for {}",
                    pmcid
                )));
            }
            format!("# {}\n\n## Abstract\n\n{}", title, abstract_text)
        } else {
            format!(
                "# {}\n\n## Abstract\n\n{}\n\n## Full Text\n\n{}",
                title, abstract_text, body_text
            )
        };

        Ok(PaperFetchResponse {
            paper_id: pmcid.clone(),
            title,
            content,
            url: format!("https://pmc.ncbi.nlm.nih.gov/articles/{}/", pmcid),
            source: "pmc".into(),
        })
    }
}

fn normalize_pmcid(id: &str) -> String {
    if id.to_uppercase().starts_with("PMC") {
        id.to_string()
    } else {
        format!("PMC{}", id)
    }
}

fn extract_tag(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{}", tag);
    let close = format!("</{}>", tag);
    let start = xml.find(&open)?;
    let after = xml[start..].find('>')? + start + 1;
    let end = xml[after..].find(&close)? + after;
    Some(strip_xml_tags(&xml[after..end]).trim().to_string())
}

/// Extract all text from within a top-level JATS section (<abstract> or <body>).
fn extract_body_text(xml: &str, section: &str) -> String {
    let open = format!("<{}", section);
    let close = format!("</{}>", section);
    let start = match xml.find(&open) {
        Some(s) => s,
        None => return String::new(),
    };
    let after = match xml[start..].find('>') {
        Some(s) => start + s + 1,
        None => return String::new(),
    };
    let end = match xml[after..].find(&close) {
        Some(s) => after + s,
        None => return String::new(),
    };
    let block = &xml[after..end];

    let mut parts = Vec::new();
    let mut remaining = block;
    while !remaining.is_empty() {
        if remaining.starts_with("<title>") {
            if let Some(e) = remaining.find("</title>") {
                let t = strip_xml_tags(&remaining[7..e]).trim().to_string();
                if !t.is_empty() {
                    parts.push(format!("\n### {}\n", t));
                }
                remaining = &remaining[e + 8..];
                continue;
            }
        }
        if remaining.starts_with("<p") {
            if let Some(gt) = remaining.find('>') {
                if let Some(close_p) = remaining.find("</p>") {
                    let text = strip_xml_tags(&remaining[gt + 1..close_p])
                        .trim()
                        .to_string();
                    if !text.is_empty() {
                        parts.push(text);
                    }
                    remaining = &remaining[close_p + 4..];
                    continue;
                }
            }
        }
        // Advance to next '<' or end
        remaining = match remaining[1..].find('<') {
            Some(i) => &remaining[i + 1..],
            None => break,
        };
    }
    parts.join("\n\n")
}

fn strip_xml_tags(s: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(c),
            _ => {}
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_name() {
        let p = PmcProvider::new(None);
        assert_eq!(p.name(), "pmc");
        assert!(!p.supports_search());
        assert!(p.supports_fetch());
    }

    #[test]
    fn test_normalize_pmcid() {
        assert_eq!(normalize_pmcid("PMC6267067"), "PMC6267067");
        assert_eq!(normalize_pmcid("6267067"), "PMC6267067");
        assert_eq!(normalize_pmcid("pmc123"), "pmc123");
    }

    #[test]
    fn test_strip_xml_tags() {
        assert_eq!(strip_xml_tags("<b>bold</b> text"), "bold text");
        assert_eq!(strip_xml_tags("plain"), "plain");
    }

    #[test]
    fn test_extract_tag() {
        let xml = r#"<article-title>My Paper Title</article-title>"#;
        assert_eq!(
            extract_tag(xml, "article-title"),
            Some("My Paper Title".into())
        );
    }

    #[test]
    fn test_extract_body_text() {
        let xml = r#"<abstract><sec><title>Background</title><p id="p1">Some background text.</p></sec><sec><title>Results</title><p>Some results.</p></sec></abstract>"#;
        let text = extract_body_text(xml, "abstract");
        assert!(text.contains("### Background"));
        assert!(text.contains("Some background text."));
        assert!(text.contains("### Results"));
        assert!(text.contains("Some results."));
    }

    #[tokio::test]
    async fn test_search_not_supported() {
        let p = PmcProvider::new(None);
        assert!(p.search("anything", 5).await.is_err());
    }

    #[tokio::test]
    #[ignore] // hits real PMC API
    async fn test_fetch_integration() {
        let p = PmcProvider::new(None);
        let result = p
            .fetch(&PaperId {
                pmcid: Some("PMC6267067".into()),
                ..Default::default()
            })
            .await;
        assert!(result.is_ok(), "fetch failed: {:?}", result);
        let resp = result.unwrap();
        assert!(resp.title.contains("PanoromiX"));
        assert!(resp.content.contains("Full Text"));
        assert!(resp.content.contains("Abstract"));
        assert_eq!(resp.source, "pmc");
    }

    #[tokio::test]
    #[ignore] // hits real PMC API
    async fn test_fetch_numeric_id() {
        let p = PmcProvider::new(None);
        let result = p
            .fetch(&PaperId {
                pmcid: Some("6267067".into()),
                ..Default::default()
            })
            .await;
        assert!(result.is_ok(), "fetch failed: {:?}", result);
        let resp = result.unwrap();
        assert!(resp.paper_id.starts_with("PMC"));
    }
}
