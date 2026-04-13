//! Provider module for web search services.

pub mod minimax;
pub mod tavily;
pub mod trait_def;
pub mod zhipu;

pub use trait_def::{FetchResponse, SearchResponse, SearchResult, WebSearchProvider};