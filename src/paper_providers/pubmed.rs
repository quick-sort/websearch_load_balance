//! PubMed paper search provider using NCBI E-utilities.
//! Supports fetch: returns abstract via efetch, and full text via PMC OA if PMCID exists.

use crate::error::WebSearchError;
use crate::paper_providers::trait_def::{
    PaperFetchResponse, PaperId, PaperResult, PaperSearchProvider, PaperSearchResponse,
};
use async_trait::async_trait;
use reqwest::Client;

const SEARCH_URL: &str = "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esearch.fcgi";
const FETCH_URL: &str = "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/efetch.fcgi";
const PMC_OA_URL: &str = "https://www.ncbi.nlm.nih.gov/research/bionlp/RESTful/pmcoa.cgi/BioC_xml";

pub struct PubmedProvider {
    client: Client,
    api_key: Option<String>,
}

impl PubmedProvider {
    pub fn new(api_key: Option<String>) -> Self {
        Self {
            client: Client::new(),
            api_key,
        }
    }

    fn api_key_params(&self) -> Vec<(&str, String)> {
        self.api_key
            .as_ref()
            .map(|k| vec![("api_key", k.clone())])
            .unwrap_or_default()
    }

    /// Fetch article XML from efetch and parse it.
    async fn fetch_article_xml(&self, pmid: &str) -> Result<String, WebSearchError> {
        let mut params = vec![
            ("db", "pubmed".to_string()),
            ("id", pmid.to_string()),
            ("retmode", "xml".to_string()),
        ];
        for (k, v) in self.api_key_params() {
            params.push((k, v));
        }
        let resp = self.client.get(FETCH_URL).query(&params).send().await?;
        Ok(resp.text().await?)
    }

    /// Try to get PMC full text via BioC API. Returns None if no PMCID or not in OA.
    async fn fetch_pmc_fulltext(&self, pmcid: &str) -> Option<String> {
        let url = format!("{}/{}/unicode", PMC_OA_URL, pmcid);
        let resp = self.client.get(&url).send().await.ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let text = resp.text().await.ok()?;
        // Extract all <text> content from BioC XML as the full text
        let parts: Vec<String> = text
            .split("<text>")
            .skip(1)
            .filter_map(|s| s.split("</text>").next().map(|t| t.trim().to_string()))
            .filter(|t| !t.is_empty())
            .collect();
        if parts.is_empty() {
            None
        } else {
            Some(parts.join("\n\n"))
        }
    }
}

#[async_trait]
impl PaperSearchProvider for PubmedProvider {
    fn name(&self) -> &str {
        "pubmed"
    }

    fn supports_fetch(&self) -> bool {
        true
    }

    async fn search(
        &self,
        query: &str,
        max_results: u32,
    ) -> Result<PaperSearchResponse, WebSearchError> {
        let mut params = vec![
            ("db", "pubmed".to_string()),
            ("term", query.to_string()),
            ("retmax", max_results.to_string()),
            ("retmode", "xml".to_string()),
        ];
        for (k, v) in self.api_key_params() {
            params.push((k, v));
        }

        let search_resp = self.client.get(SEARCH_URL).query(&params).send().await?;
        let search_xml = search_resp.text().await?;

        let ids: Vec<String> = search_xml
            .split("<Id>")
            .skip(1)
            .filter_map(|s| s.split("</Id>").next().map(|id| id.trim().to_string()))
            .collect();

        if ids.is_empty() {
            return Ok(PaperSearchResponse { papers: vec![] });
        }

        let mut fetch_params = vec![
            ("db", "pubmed".to_string()),
            ("id", ids.join(",")),
            ("retmode", "xml".to_string()),
        ];
        for (k, v) in self.api_key_params() {
            fetch_params.push((k, v));
        }

        let fetch_resp = self
            .client
            .get(FETCH_URL)
            .query(&fetch_params)
            .send()
            .await?;
        let fetch_xml = fetch_resp.text().await?;

        let papers = parse_pubmed_xml(&fetch_xml);
        Ok(PaperSearchResponse { papers })
    }

    async fn fetch(&self, id: &PaperId) -> Result<PaperFetchResponse, WebSearchError> {
        // Prefer PMID, fall back to PMCID
        let identifier =
            id.pmid.as_deref().or(id.pmcid.as_deref()).ok_or_else(|| {
                WebSearchError::NotSupported("pubmed requires pmid or pmcid".into())
            })?;

        let is_pmcid = identifier.to_uppercase().starts_with("PMC");

        if is_pmcid {
            // Try PMC full text directly
            if let Some(fulltext) = self.fetch_pmc_fulltext(identifier).await {
                return Ok(PaperFetchResponse {
                    paper_id: identifier.to_string(),
                    title: String::new(),
                    content: fulltext,
                    url: format!("https://www.ncbi.nlm.nih.gov/pmc/articles/{}/", identifier),
                    source: "pubmed".into(),
                });
            }
            return Err(WebSearchError::ParseError(format!(
                "Could not fetch full text for {}",
                identifier
            )));
        }

        // PMID path: fetch article metadata + abstract, then try PMC full text if PMCID exists
        let xml = self.fetch_article_xml(identifier).await?;
        let articles = parse_pubmed_xml(&xml);
        let article = articles.into_iter().next().ok_or_else(|| {
            WebSearchError::ParseError(format!("Article not found: {}", identifier))
        })?;

        // Try to extract PMCID from the XML
        let pmcid = extract_pmcid(&xml);

        // Try PMC full text if PMCID available
        let fulltext = if let Some(ref pmc) = pmcid {
            self.fetch_pmc_fulltext(pmc).await
        } else {
            None
        };

        let content = if let Some(ft) = fulltext {
            format!(
                "Title: {}\nAuthors: {}\nDOI: {}\nPMCID: {}\n\n--- Full Text ---\n\n{}",
                article.title,
                article.authors.join(", "),
                article.id.doi.as_deref().unwrap_or("N/A"),
                pmcid.as_deref().unwrap_or("N/A"),
                ft
            )
        } else {
            format!(
                "Title: {}\nAuthors: {}\nDOI: {}\n\n--- Abstract ---\n\n{}",
                article.title,
                article.authors.join(", "),
                article.id.doi.as_deref().unwrap_or("N/A"),
                article.r#abstract
            )
        };

        Ok(PaperFetchResponse {
            paper_id: article.id.best_id().to_string(),
            title: article.title,
            content,
            url: article.id.url.unwrap_or_default(),
            source: "pubmed".into(),
        })
    }
}

/// Extract PMCID from efetch XML (appears in <ArticleId IdType="pmc">).
fn extract_pmcid(xml: &str) -> Option<String> {
    let marker = "IdType=\"pmc\"";
    let pos = xml.find(marker)?;
    let after = &xml[pos + marker.len()..];
    let start = after.find('>')? + 1;
    let end = after[start..].find('<')? + start;
    let id = after[start..end].trim();
    if id.is_empty() {
        None
    } else if id.to_uppercase().starts_with("PMC") {
        Some(id.to_string())
    } else {
        Some(format!("PMC{}", id))
    }
}

fn parse_pubmed_xml(xml: &str) -> Vec<PaperResult> {
    let mut papers = Vec::new();
    for article_block in xml.split("<PubmedArticle>").skip(1) {
        let pmid = extract_tag(article_block, "PMID").unwrap_or_default();
        let title = extract_tag(article_block, "ArticleTitle").unwrap_or_default();
        if title.is_empty() {
            continue;
        }

        let abstract_text = extract_tag(article_block, "AbstractText").unwrap_or_default();
        let year = extract_tag(article_block, "Year").unwrap_or_default();

        let authors: Vec<String> = article_block
            .split("<Author")
            .skip(1)
            .filter_map(|a| {
                let last = extract_tag(a, "LastName")?;
                let initials = extract_tag(a, "Initials").unwrap_or_default();
                Some(format!("{} {}", last, initials))
            })
            .collect();

        let doi = article_block
            .split("EIdType=\"doi\"")
            .nth(1)
            .and_then(|s| s.split('>').nth(1))
            .and_then(|s| s.split('<').next())
            .map(|s| s.trim().to_string());

        // Extract PMCID for pdf_url
        let pmcid = extract_pmcid(article_block);
        let pdf_url = pmcid
            .as_ref()
            .map(|pmc| format!("https://www.ncbi.nlm.nih.gov/pmc/articles/{}/pdf/", pmc));

        // Extract PII
        let pii = extract_article_id(article_block, "pii");

        papers.push(PaperResult {
            id: PaperId {
                doi,
                pmid: Some(pmid.clone()),
                pmcid: pmcid.clone(),
                url: Some(format!("https://pubmed.ncbi.nlm.nih.gov/{}/", pmid)),
                pii,
                ..Default::default()
            },
            title,
            authors,
            r#abstract: abstract_text,
            pdf_url,
            published_date: if year.is_empty() { None } else { Some(year) },
            source: "pubmed".into(),
            categories: vec![],
            citations: None,
        });
    }
    papers
}

fn extract_tag(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{}", tag);
    let close = format!("</{}>", tag);
    let start = xml.find(&open)?;
    let after_open = xml[start..].find('>')? + start + 1;
    let end = xml[after_open..].find(&close)? + after_open;
    Some(xml[after_open..end].trim().to_string())
}

/// Extract ArticleId by IdType (e.g. "pii", "pmc", "doi") from ArticleIdList.
fn extract_article_id(xml: &str, id_type: &str) -> Option<String> {
    let marker = format!("IdType=\"{}\"", id_type);
    let pos = xml.find(&marker)?;
    let after = &xml[pos + marker.len()..];
    let start = after.find('>')? + 1;
    let end = after[start..].find('<')? + start;
    let id = after[start..end].trim();
    if id.is_empty() {
        None
    } else {
        Some(id.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_name() {
        let p = PubmedProvider::new(None);
        assert_eq!(p.name(), "pubmed");
        assert!(p.supports_fetch());
        assert!(p.supports_search());
    }

    #[test]
    fn test_extract_pmcid() {
        let xml = r#"<ArticleId IdType="pmc">PMC1234567</ArticleId>"#;
        assert_eq!(extract_pmcid(xml), Some("PMC1234567".into()));

        let xml2 = r#"<ArticleId IdType="pmc">1234567</ArticleId>"#;
        assert_eq!(extract_pmcid(xml2), Some("PMC1234567".into()));

        let xml3 = r#"<ArticleId IdType="doi">10.1234</ArticleId>"#;
        assert_eq!(extract_pmcid(xml3), None);
    }

    #[test]
    fn test_parse_pubmed_xml() {
        let xml = r#"<PubmedArticleSet>
<PubmedArticle>
  <MedlineCitation>
    <PMID>12345678</PMID>
    <Article>
      <ArticleTitle>A study on CRISPR gene editing</ArticleTitle>
      <Abstract><AbstractText>We investigated CRISPR-Cas9 efficiency.</AbstractText></Abstract>
      <AuthorList>
        <Author><LastName>Zhang</LastName><Initials>F</Initials></Author>
        <Author><LastName>Doudna</LastName><Initials>JA</Initials></Author>
      </AuthorList>
      <Journal><JournalIssue><PubDate><Year>2023</Year></PubDate></JournalIssue></Journal>
      <ELocationID EIdType="doi">10.1234/test.2023</ELocationID>
    </Article>
  </MedlineCitation>
  <PubmedData>
    <ArticleIdList>
      <ArticleId IdType="pmc">PMC9999999</ArticleId>
    </ArticleIdList>
  </PubmedData>
</PubmedArticle>
</PubmedArticleSet>"#;
        let papers = parse_pubmed_xml(xml);
        assert_eq!(papers.len(), 1);
        let p = &papers[0];
        assert_eq!(p.id.pmid.as_deref(), Some("12345678"));
        assert!(p.title.contains("CRISPR"));
        assert_eq!(p.authors, vec!["Zhang F", "Doudna JA"]);
        assert!(p.r#abstract.contains("CRISPR-Cas9"));
        assert_eq!(p.published_date.as_deref(), Some("2023"));
        assert_eq!(p.id.doi.as_deref(), Some("10.1234/test.2023"));
        assert_eq!(p.source, "pubmed");
        assert_eq!(
            p.pdf_url.as_deref(),
            Some("https://www.ncbi.nlm.nih.gov/pmc/articles/PMC9999999/pdf/")
        );
        assert_eq!(
            p.id.url.as_deref(),
            Some("https://pubmed.ncbi.nlm.nih.gov/12345678/")
        );
    }

    #[test]
    fn test_parse_pubmed_xml_empty() {
        assert!(parse_pubmed_xml("<PubmedArticleSet></PubmedArticleSet>").is_empty());
    }

    #[test]
    fn test_parse_pubmed_xml_no_abstract() {
        let xml = r#"<PubmedArticle>
  <MedlineCitation>
    <PMID>111</PMID>
    <Article>
      <ArticleTitle>Short note</ArticleTitle>
      <Journal><JournalIssue><PubDate><Year>2020</Year></PubDate></JournalIssue></Journal>
    </Article>
  </MedlineCitation>
</PubmedArticle>"#;
        let papers = parse_pubmed_xml(xml);
        assert_eq!(papers.len(), 1);
        assert!(papers[0].r#abstract.is_empty());
        assert!(papers[0].pdf_url.is_none());
    }

    #[test]
    fn test_extract_tag() {
        assert_eq!(
            extract_tag("<Year>2023</Year>", "Year"),
            Some("2023".into())
        );
        assert_eq!(
            extract_tag("<PMID Version=\"1\">999</PMID>", "PMID"),
            Some("999".into())
        );
        assert_eq!(extract_tag("<Foo>bar</Foo>", "Year"), None);
    }

    #[tokio::test]
    #[ignore] // hits real PubMed API
    async fn test_search_integration() {
        let p = PubmedProvider::new(None);
        let result = p.search("CRISPR", 3).await;
        assert!(result.is_ok(), "search failed: {:?}", result);
        // PubMed may occasionally return empty due to rate limiting
        let resp = result.unwrap();
        if resp.papers.is_empty() {
            return;
        }
        let first = &resp.papers[0];
        assert!(!first.title.is_empty());
        assert!(!first.id.is_empty());
        assert_eq!(first.source, "pubmed");
    }

    #[tokio::test]
    #[ignore] // hits real PubMed API
    async fn test_fetch_abstract_integration() {
        let p = PubmedProvider::new(None);
        // PMID 19872477 = a well-known PubMed article
        let result = p
            .fetch(&PaperId {
                pmid: Some("19872477".into()),
                ..Default::default()
            })
            .await;
        assert!(result.is_ok(), "fetch failed: {:?}", result);
        let resp = result.unwrap();
        assert!(!resp.content.is_empty());
        assert_eq!(resp.source, "pubmed");
    }
}
