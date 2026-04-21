//! Standalone test to dump each paper provider's output to files for inspection.
//! Run with: cargo test --test dump_provider_results -- --ignored --nocapture

use one_search::paper_providers::arxiv::ArxivProvider;
use one_search::paper_providers::biorxiv::BiorxivProvider;
use one_search::paper_providers::google_scholar::GoogleScholarProvider;
use one_search::paper_providers::medrxiv::MedrxivProvider;
use one_search::paper_providers::pmc::PmcProvider;
use one_search::paper_providers::pubmed::PubmedProvider;
use one_search::paper_providers::sci_hub::SciHubProvider;
use one_search::paper_providers::semantic::SemanticProvider;
use one_search::paper_providers::trait_def::{PaperId, PaperSearchProvider};
use std::fs;

const OUT_DIR: &str = "test_output";

async fn dump_search(provider: &dyn PaperSearchProvider, query: &str) {
    let name = provider.name();
    if !provider.supports_search() {
        fs::write(
            format!("{}/{}_search.txt", OUT_DIR, name),
            format!("[{}] does not support search", name),
        )
        .unwrap();
        return;
    }
    match provider.search(query, 3).await {
        Ok(resp) => {
            let json = serde_json::to_string_pretty(&resp).unwrap();
            fs::write(format!("{}/{}_search.json", OUT_DIR, name), &json).unwrap();
            println!("[{}] search: {} papers saved", name, resp.papers.len());
        }
        Err(e) => {
            let msg = format!("[{}] search error: {}", name, e);
            fs::write(format!("{}/{}_search_error.txt", OUT_DIR, name), &msg).unwrap();
            println!("{}", msg);
        }
    }
}

async fn dump_fetch(provider: &dyn PaperSearchProvider, id: &PaperId, label: &str) {
    let name = provider.name();
    if !provider.supports_fetch() {
        fs::write(
            format!("{}/{}_fetch.txt", OUT_DIR, name),
            format!("[{}] does not support fetch", name),
        )
        .unwrap();
        return;
    }
    match provider.fetch(id).await {
        Ok(resp) => {
            let json = serde_json::to_string_pretty(&resp).unwrap();
            fs::write(format!("{}/{}_fetch.json", OUT_DIR, name), &json).unwrap();
            println!("[{}] fetch ({}): saved", name, label);
        }
        Err(e) => {
            let msg = format!("[{}] fetch ({}) error: {}", name, label, e);
            fs::write(format!("{}/{}_fetch_error.txt", OUT_DIR, name), &msg).unwrap();
            println!("{}", msg);
        }
    }
}

#[tokio::test]
#[ignore]
async fn dump_all_providers() {
    fs::create_dir_all(OUT_DIR).unwrap();

    let query = "CRISPR gene editing";

    // --- Search providers ---
    let google_scholar = GoogleScholarProvider::new();
    let pubmed = PubmedProvider::new(None);
    let arxiv = ArxivProvider::new();
    let biorxiv = BiorxivProvider::new();
    let medrxiv = MedrxivProvider::new();
    let semantic = SemanticProvider::new(None);
    let sci_hub = SciHubProvider::new(None);
    let pmc = PmcProvider::new(None);

    // Search
    dump_search(&google_scholar, query).await;
    dump_search(&pubmed, query).await;
    dump_search(&arxiv, query).await;
    dump_search(&biorxiv, "genomics").await; // biorxiv uses category
    dump_search(&medrxiv, "epidemiology").await; // medrxiv uses category
    dump_search(&semantic, query).await;
    dump_search(&sci_hub, query).await;
    dump_search(&pmc, query).await;

    // Fetch
    dump_fetch(
        &arxiv,
        &PaperId {
            arxiv_id: Some("2106.09685".into()),
            ..Default::default()
        },
        "LoRA paper",
    )
    .await;
    dump_fetch(
        &semantic,
        &PaperId {
            semantic_id: Some("204e3073870fae3d05bcbc2f6a8e263d9b72e776".into()),
            ..Default::default()
        },
        "Attention paper",
    )
    .await;
    dump_fetch(
        &pubmed,
        &PaperId {
            pmid: Some("19872477".into()),
            ..Default::default()
        },
        "PMID 19872477",
    )
    .await;
    dump_fetch(
        &pmc,
        &PaperId {
            pmcid: Some("PMC6267067".into()),
            ..Default::default()
        },
        "PMC6267067",
    )
    .await;
    dump_fetch(
        &sci_hub,
        &PaperId {
            doi: Some("10.1038/nature12373".into()),
            ..Default::default()
        },
        "DOI",
    )
    .await;

    println!("\n=== All results saved to {}/  ===", OUT_DIR);
}
