//! Sci-Hub paper fetch provider (fetch-only, no search).

use crate::error::WebSearchError;
use crate::paper_providers::trait_def::{
    PaperFetchResponse, PaperId, PaperSearchProvider, PaperSearchResponse,
};
use async_trait::async_trait;
use reqwest::Client;

pub struct SciHubProvider {
    client: Client,
    base_url: String,
}

impl SciHubProvider {
    pub fn new(base_url: Option<String>) -> Self {
        Self {
            client: Client::builder()
                .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
                .danger_accept_invalid_certs(true)
                .build()
                .unwrap_or_default(),
            base_url: base_url.unwrap_or_else(|| "https://sci-hub.se".into()),
        }
    }
}

#[async_trait]
impl PaperSearchProvider for SciHubProvider {
    fn name(&self) -> &str {
        "sci_hub"
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
            "Sci-Hub does not support search, only fetch by DOI/URL".into(),
        ))
    }

    async fn fetch(&self, id: &PaperId) -> Result<PaperFetchResponse, WebSearchError> {
        // Sci-Hub works best with DOI, also accepts URL
        let identifier = id
            .doi
            .as_deref()
            .or(id.url.as_deref())
            .or(id.pmid.as_deref())
            .ok_or_else(|| {
                WebSearchError::NotSupported("sci_hub requires doi, url, or pmid".into())
            })?;
        let url = format!("{}/{}", self.base_url, identifier);
        let resp = self.client.get(&url).send().await?;

        if !resp.status().is_success() {
            return Err(WebSearchError::ProviderError(
                resp.status().as_u16() as i32,
                "Sci-Hub request failed".into(),
            ));
        }

        let html = resp.text().await?;

        // Check if article was found
        if html.to_lowercase().contains("article not found") {
            return Err(WebSearchError::ParseError(format!(
                "Article not found on Sci-Hub: {}",
                identifier
            )));
        }

        // Extract PDF URL from embed or iframe
        let pdf_url = extract_pdf_url(&html, &self.base_url);

        Ok(PaperFetchResponse {
            paper_id: identifier.to_string(),
            title: extract_title(&html).unwrap_or_else(|| identifier.to_string()),
            content: if let Some(ref pdf) = pdf_url {
                format!(
                    "PDF available at: {}\nAccess via Sci-Hub: {}/{}",
                    pdf, self.base_url, identifier
                )
            } else {
                format!(
                    "Page loaded but PDF URL not found. Access via: {}/{}",
                    self.base_url, identifier
                )
            },
            url: pdf_url.unwrap_or_else(|| format!("{}/{}", self.base_url, identifier)),
            source: "sci_hub".into(),
        })
    }
}

fn extract_pdf_url(html: &str, base_url: &str) -> Option<String> {
    // Try embed tag
    if let Some(src) = extract_attr(html, "embed", "src") {
        return Some(normalize_url(&src, base_url));
    }
    // Try iframe
    if let Some(src) = extract_attr(html, "iframe", "src") {
        return Some(normalize_url(&src, base_url));
    }
    None
}

fn extract_attr(html: &str, tag: &str, attr: &str) -> Option<String> {
    let tag_start = html.find(&format!("<{}", tag))?;
    let tag_end = html[tag_start..].find('>')? + tag_start;
    let tag_content = &html[tag_start..tag_end];
    let attr_pattern = format!("{}=\"", attr);
    let attr_start = tag_content.find(&attr_pattern)? + attr_pattern.len();
    let attr_end = tag_content[attr_start..].find('"')? + attr_start;
    Some(tag_content[attr_start..attr_end].to_string())
}

fn normalize_url(url: &str, base_url: &str) -> String {
    if url.starts_with("//") {
        format!("https:{}", url)
    } else if url.starts_with('/') {
        format!("{}{}", base_url, url)
    } else {
        url.to_string()
    }
}

fn extract_title(html: &str) -> Option<String> {
    let start = html.find("<title>")? + 7;
    let end = html[start..].find("</title>")? + start;
    let title = html[start..end].trim().to_string();
    if title.is_empty() {
        None
    } else {
        Some(title)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_name() {
        let p = SciHubProvider::new(None);
        assert_eq!(p.name(), "sci_hub");
        assert!(!p.supports_search());
        assert!(p.supports_fetch());
    }

    #[test]
    fn test_custom_base_url() {
        let p = SciHubProvider::new(Some("https://sci-hub.st".into()));
        assert_eq!(p.base_url, "https://sci-hub.st");
    }

    #[test]
    fn test_normalize_url() {
        assert_eq!(
            normalize_url("//cdn.example.com/paper.pdf", "https://sci-hub.se"),
            "https://cdn.example.com/paper.pdf"
        );
        assert_eq!(
            normalize_url("/downloads/paper.pdf", "https://sci-hub.se"),
            "https://sci-hub.se/downloads/paper.pdf"
        );
        assert_eq!(
            normalize_url("https://full.url/paper.pdf", "https://sci-hub.se"),
            "https://full.url/paper.pdf"
        );
    }

    #[test]
    fn test_extract_pdf_url_embed() {
        let html = r#"<html><embed src="//moscow.sci-hub.se/123/paper.pdf" type="application/pdf"></html>"#;
        assert_eq!(
            extract_pdf_url(html, "https://sci-hub.se"),
            Some("https://moscow.sci-hub.se/123/paper.pdf".into())
        );
    }

    #[test]
    fn test_extract_pdf_url_iframe() {
        let html = r#"<html><iframe src="/downloads/paper.pdf"></iframe></html>"#;
        assert_eq!(
            extract_pdf_url(html, "https://sci-hub.se"),
            Some("https://sci-hub.se/downloads/paper.pdf".into())
        );
    }

    #[test]
    fn test_extract_pdf_url_none() {
        let html = r#"<html><body>No PDF here</body></html>"#;
        assert_eq!(extract_pdf_url(html, "https://sci-hub.se"), None);
    }

    #[test]
    fn test_extract_title() {
        assert_eq!(
            extract_title("<html><title>My Paper</title></html>"),
            Some("My Paper".into())
        );
        assert_eq!(extract_title("<html><title></title></html>"), None);
        assert_eq!(extract_title("<html></html>"), None);
    }

    #[test]
    fn test_extract_attr() {
        let html = r#"<embed src="test.pdf" type="application/pdf">"#;
        assert_eq!(extract_attr(html, "embed", "src"), Some("test.pdf".into()));
        assert_eq!(
            extract_attr(html, "embed", "type"),
            Some("application/pdf".into())
        );
        assert_eq!(extract_attr(html, "embed", "missing"), None);
        assert_eq!(extract_attr(html, "iframe", "src"), None);
    }

    #[tokio::test]
    #[ignore] // hits real Sci-Hub
    async fn test_fetch_integration() {
        let p = SciHubProvider::new(None);
        let result = p
            .fetch(&PaperId {
                doi: Some("10.1038/nature12373".into()),
                ..Default::default()
            })
            .await;
        assert!(result.is_ok(), "fetch failed: {:?}", result);
        let resp = result.unwrap();
        assert_eq!(resp.source, "sci_hub");
        assert!(!resp.content.is_empty());
    }

    #[tokio::test]
    async fn test_search_not_supported() {
        let p = SciHubProvider::new(None);
        let result = p.search("anything", 5).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            WebSearchError::NotSupported(_) => {}
            other => panic!("expected NotSupported, got: {}", other),
        }
    }
}
