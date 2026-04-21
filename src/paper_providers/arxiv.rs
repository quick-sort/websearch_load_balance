//! arXiv paper search provider using the arXiv API.

use crate::error::WebSearchError;
use crate::paper_providers::trait_def::{
    PaperFetchResponse, PaperId, PaperResult, PaperSearchProvider, PaperSearchResponse,
};
use async_trait::async_trait;
use reqwest::Client;

const BASE_URL: &str = "http://export.arxiv.org/api/query";

pub struct ArxivProvider {
    client: Client,
}

impl ArxivProvider {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }
}

#[async_trait]
impl PaperSearchProvider for ArxivProvider {
    fn name(&self) -> &str {
        "arxiv"
    }

    fn supports_fetch(&self) -> bool {
        true
    }

    async fn search(
        &self,
        query: &str,
        max_results: u32,
    ) -> Result<PaperSearchResponse, WebSearchError> {
        let resp = self
            .client
            .get(BASE_URL)
            .query(&[
                ("search_query", format!("all:{}", query)),
                ("max_results", max_results.to_string()),
                ("sortBy", "submittedDate".into()),
                ("sortOrder", "descending".into()),
            ])
            .send()
            .await?;

        let xml = resp.text().await?;
        let papers = parse_arxiv_atom(&xml);
        Ok(PaperSearchResponse { papers })
    }

    async fn fetch(&self, id: &PaperId) -> Result<PaperFetchResponse, WebSearchError> {
        let arxiv_id = id
            .arxiv_id
            .as_deref()
            .or(id.url.as_deref())
            .ok_or_else(|| WebSearchError::NotSupported("arxiv requires arxiv_id".into()))?;

        let abs_url = if arxiv_id.starts_with("http") {
            arxiv_id.to_string()
        } else {
            format!("http://export.arxiv.org/api/query?id_list={}", arxiv_id)
        };

        let resp = self.client.get(&abs_url).send().await?;
        let xml = resp.text().await?;
        let papers = parse_arxiv_atom(&xml);

        let paper = papers
            .into_iter()
            .next()
            .ok_or_else(|| WebSearchError::ParseError(format!("Paper not found: {}", arxiv_id)))?;

        Ok(PaperFetchResponse {
            paper_id: paper.id.best_id().to_string(),
            title: paper.title.clone(),
            content: format!(
                "Title: {}\nAuthors: {}\nAbstract: {}\nCategories: {}",
                paper.title,
                paper.authors.join(", "),
                paper.r#abstract,
                paper.categories.join(", ")
            ),
            url: paper.id.url.unwrap_or_default(),
            source: "arxiv".into(),
        })
    }
}

fn parse_arxiv_atom(xml: &str) -> Vec<PaperResult> {
    let mut papers = Vec::new();
    for entry in xml.split("<entry>").skip(1) {
        let title = extract_tag_content(entry, "title")
            .map(|s| s.replace('\n', " ").trim().to_string())
            .unwrap_or_default();
        if title.is_empty() {
            continue;
        }

        let id_full = extract_tag_content(entry, "id").unwrap_or_default();
        let paper_id = id_full.rsplit('/').next().unwrap_or("").to_string();

        let summary = extract_tag_content(entry, "summary")
            .map(|s| s.replace('\n', " ").trim().to_string())
            .unwrap_or_default();

        let published =
            extract_tag_content(entry, "published").map(|s| s.chars().take(10).collect::<String>());

        // Extract authors
        let authors: Vec<String> = entry
            .split("<author>")
            .skip(1)
            .filter_map(|a| extract_tag_content(a, "name"))
            .collect();

        // Extract categories
        let categories: Vec<String> = entry
            .split("term=\"")
            .skip(1)
            .filter_map(|s| s.split('"').next().map(|t| t.to_string()))
            .collect();

        // Extract PDF link
        let pdf_url = entry
            .split("title=\"pdf\"")
            .nth(1)
            .or_else(|| entry.split("type=\"application/pdf\"").nth(1))
            .and_then(|s| {
                // look backwards for href="
                let before = &entry[..entry.len() - s.len()];
                before.rfind("href=\"").map(|i| {
                    let start = i + 6;
                    before[start..].split('"').next().unwrap_or("").to_string()
                })
            })
            .or_else(|| Some(format!("https://arxiv.org/pdf/{}", paper_id)));

        // Extract DOI
        let doi = extract_tag_content(entry, "arxiv:doi");

        papers.push(PaperResult {
            id: PaperId {
                doi,
                arxiv_id: Some(paper_id),
                url: Some(id_full),
                ..Default::default()
            },
            title,
            authors,
            r#abstract: summary,
            pdf_url,
            published_date: published,
            source: "arxiv".into(),
            categories,
            citations: None,
        });
    }
    papers
}

fn extract_tag_content(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{}", tag);
    let close = format!("</{}>", tag);
    let start = xml.find(&open)?;
    let after_open = xml[start..].find('>')? + start + 1;
    let end = xml[after_open..].find(&close)? + after_open;
    Some(xml[after_open..end].trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_name() {
        let p = ArxivProvider::new();
        assert_eq!(p.name(), "arxiv");
        assert!(p.supports_fetch());
        assert!(p.supports_search());
    }

    #[test]
    fn test_parse_arxiv_atom() {
        let xml = r#"<?xml version="1.0"?>
<feed xmlns="http://www.w3.org/2005/Atom">
<entry>
  <id>http://arxiv.org/abs/2106.15928v1</id>
  <title>LoRA: Low-Rank Adaptation of Large Language Models</title>
  <summary>We propose LoRA, a method for adapting large models.</summary>
  <published>2021-06-17T00:00:00Z</published>
  <author><name>Edward Hu</name></author>
  <author><name>Yelong Shen</name></author>
  <category term="cs.CL"/>
  <category term="cs.AI"/>
  <link href="http://arxiv.org/pdf/2106.15928v1" title="pdf" type="application/pdf"/>
  <arxiv:doi>10.48550/arXiv.2106.15928</arxiv:doi>
</entry>
</feed>"#;
        let papers = parse_arxiv_atom(xml);
        assert_eq!(papers.len(), 1);
        let p = &papers[0];
        assert_eq!(p.id.arxiv_id.as_deref(), Some("2106.15928v1"));
        assert!(p.title.contains("LoRA"));
        assert_eq!(p.authors, vec!["Edward Hu", "Yelong Shen"]);
        assert!(p.r#abstract.contains("LoRA"));
        assert_eq!(p.published_date.as_deref(), Some("2021-06-17"));
        assert_eq!(p.source, "arxiv");
        assert!(p.categories.contains(&"cs.CL".to_string()));
        assert!(p.categories.contains(&"cs.AI".to_string()));
        assert_eq!(p.id.doi.as_deref(), Some("10.48550/arXiv.2106.15928"));
    }

    #[test]
    fn test_parse_arxiv_atom_empty() {
        let xml = r#"<?xml version="1.0"?><feed></feed>"#;
        assert!(parse_arxiv_atom(xml).is_empty());
    }

    #[test]
    fn test_parse_arxiv_atom_missing_title() {
        let xml = r#"<entry><id>http://arxiv.org/abs/1234</id><summary>text</summary></entry>"#;
        assert!(parse_arxiv_atom(xml).is_empty());
    }

    #[test]
    fn test_extract_tag_content() {
        assert_eq!(
            extract_tag_content("<name>Alice</name>", "name"),
            Some("Alice".into())
        );
        assert_eq!(extract_tag_content("<foo>bar</foo>", "name"), None);
        assert_eq!(
            extract_tag_content("<name attr=\"x\">Bob</name>", "name"),
            Some("Bob".into())
        );
    }

    #[tokio::test]
    #[ignore] // hits real arXiv API
    async fn test_search_integration() {
        let p = ArxivProvider::new();
        let result = p.search("machine learning", 3).await;
        assert!(result.is_ok(), "search failed: {:?}", result);
        let resp = result.unwrap();
        assert!(!resp.papers.is_empty());
        let first = &resp.papers[0];
        assert!(!first.title.is_empty());
        assert!(!first.id.is_empty());
        assert_eq!(first.source, "arxiv");
    }

    #[tokio::test]
    #[ignore] // hits real arXiv API
    async fn test_fetch_integration() {
        let p = ArxivProvider::new();
        let id = PaperId {
            arxiv_id: Some("2106.09685".into()),
            ..Default::default()
        };
        let result = p.fetch(&id).await;
        assert!(result.is_ok(), "fetch failed: {:?}", result);
        let resp = result.unwrap();
        assert!(
            resp.title.to_lowercase().contains("lora"),
            "unexpected title: {}",
            resp.title
        );
        assert!(resp.content.contains("Abstract:"));
        assert_eq!(resp.source, "arxiv");
    }
}
