//! Paper provider module for academic search services.

pub mod arxiv;
pub mod biorxiv;
pub mod google_scholar;
pub mod medrxiv;
pub mod pmc;
pub mod pubmed;
pub mod sci_hub;
pub mod semantic;
pub mod trait_def;

pub use trait_def::{
    PaperFetchResponse, PaperId, PaperResult, PaperSearchProvider, PaperSearchResponse,
};
