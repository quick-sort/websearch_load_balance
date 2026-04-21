//! Google Scholar paper search provider (scraping-based, no API key needed).

use crate::error::WebSearchError;
use crate::paper_providers::trait_def::{
    PaperId, PaperResult, PaperSearchProvider, PaperSearchResponse,
};
use async_trait::async_trait;
use reqwest::Client;

pub struct GoogleScholarProvider {
    client: Client,
}

impl GoogleScholarProvider {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
                .build()
                .unwrap_or_default(),
        }
    }
}

#[async_trait]
impl PaperSearchProvider for GoogleScholarProvider {
    fn name(&self) -> &str {
        "google_scholar"
    }

    async fn search(
        &self,
        query: &str,
        max_results: u32,
    ) -> Result<PaperSearchResponse, WebSearchError> {
        let resp = self
            .client
            .get("https://scholar.google.com/scholar")
            .query(&[
                ("q", query),
                ("hl", "en"),
                ("num", &max_results.to_string()),
            ])
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(WebSearchError::ProviderError(
                resp.status().as_u16() as i32,
                "Google Scholar request failed".into(),
            ));
        }

        let html = resp.text().await?;
        let papers = parse_scholar_html(&html);
        Ok(PaperSearchResponse { papers })
    }
}

fn parse_scholar_html(html: &str) -> Vec<PaperResult> {
    let mut papers = Vec::new();
    for block in html.split("class=\"gs_ri\"") {
        if papers.len() > 0 || block.contains("gs_rt") {
            let title = extract_between(block, "class=\"gs_rt\">", "</h3>")
                .map(|s| strip_html_tags(&s))
                .unwrap_or_default();
            if title.is_empty() {
                continue;
            }

            let url = extract_between(block, "<a href=\"", "\"").unwrap_or_default();

            let snippet = extract_between(block, "class=\"gs_rs\">", "</div>")
                .map(|s| strip_html_tags(&s))
                .unwrap_or_default();

            let info = extract_between(block, "class=\"gs_a\">", "</div>")
                .map(|s| strip_html_tags(&s))
                .unwrap_or_default();
            let authors: Vec<String> = info
                .split('-')
                .next()
                .unwrap_or("")
                .split(',')
                .map(|a| a.trim().to_string())
                .filter(|a| !a.is_empty())
                .collect();

            let year = info
                .split_whitespace()
                .find(|w| w.len() == 4 && w.parse::<u16>().is_ok())
                .map(|y| y.to_string());

            papers.push(PaperResult {
                id: PaperId {
                    url: Some(url),
                    ..Default::default()
                },
                title,
                authors,
                r#abstract: snippet,
                pdf_url: None,
                published_date: year,
                source: "google_scholar".into(),
                categories: vec![],
                citations: None,
            });
        }
    }
    papers
}

fn extract_between(text: &str, start: &str, end: &str) -> Option<String> {
    let s = text.find(start)? + start.len();
    let e = text[s..].find(end)? + s;
    Some(text[s..e].to_string())
}

fn strip_html_tags(s: &str) -> String {
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
    result.trim().to_string()
}

fn md5_hash(s: &str) -> u64 {
    let mut h: u64 = 0;
    for b in s.bytes() {
        h = h.wrapping_mul(31).wrapping_add(b as u64);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_name() {
        let p = GoogleScholarProvider::new();
        assert_eq!(p.name(), "google_scholar");
        assert!(p.supports_search());
        assert!(!p.supports_fetch());
    }

    #[test]
    fn test_strip_html_tags() {
        assert_eq!(strip_html_tags("<b>bold</b> text"), "bold text");
        assert_eq!(strip_html_tags("no tags"), "no tags");
        assert_eq!(strip_html_tags("<a href=\"x\">link</a>"), "link");
        assert_eq!(strip_html_tags(""), "");
    }

    #[test]
    fn test_extract_between() {
        assert_eq!(
            extract_between("foo<start>content<end>bar", "<start>", "<end>"),
            Some("content".into())
        );
        assert_eq!(extract_between("no match", "<a>", "<b>"), None);
        assert_eq!(
            extract_between("x<a>y<a>z<b>w", "<a>", "<b>"),
            Some("y<a>z".into())
        );
    }

    #[test]
    fn test_md5_hash_deterministic() {
        let h1 = md5_hash("https://example.com/paper1");
        let h2 = md5_hash("https://example.com/paper1");
        let h3 = md5_hash("https://example.com/paper2");
        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
    }

    #[test]
    fn test_parse_scholar_html() {
        let html = r#"<div class="gs_ri">
<h3 class="gs_rt"><a href="https://example.com/paper">Attention Is All You Need</a></h3>
<div class="gs_a">A Vaswani, N Shazeer - Advances in neural information processing, 2017</div>
<div class="gs_rs">We propose a new architecture based on attention mechanisms.</div>
</div>"#;
        let papers = parse_scholar_html(html);
        assert_eq!(papers.len(), 1);
        let p = &papers[0];
        assert!(p.title.contains("Attention Is All You Need"));
        assert_eq!(p.id.url.as_deref(), Some("https://example.com/paper"));
        assert!(p.r#abstract.contains("attention mechanisms"));
        assert!(p.authors.iter().any(|a| a.contains("Vaswani")));
        assert_eq!(p.published_date.as_deref(), Some("2017"));
        assert_eq!(p.source, "google_scholar");
    }

    #[test]
    fn test_parse_scholar_html_empty() {
        assert!(parse_scholar_html("<html><body></body></html>").is_empty());
    }

    #[tokio::test]
    #[ignore] // hits real Google Scholar (may be rate-limited/blocked)
    async fn test_search_integration() {
        let p = GoogleScholarProvider::new();
        let result = p.search("transformer neural network", 3).await;
        assert!(result.is_ok(), "search failed: {:?}", result);
    }
}
