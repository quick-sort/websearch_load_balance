# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Common Commands

```bash
cargo build              # Debug build
cargo build --release    # Release build (optimized)
cargo test              # Unit tests only
cargo test -- --ignored  # Integration tests (requires API keys)
cargo fmt              # Format code
cargo clippy           # Lint with warnings as errors
cargo clippy -- -D warnings  # Treat warnings as errors
```

## Architecture

### Overview
An MCP server that provides `web_search` and `web_fetch` tools with load balancing across multiple providers and API keys.

```
WebSearchMcpServer (tools/server.rs)
    └── ProviderLoadBalancer (load_balancer/manager.rs)
        ├── SelectionStrategy (load_balancer/strategy.rs)
        │   ├── RoundRobinStrategy (thread-safe atomic counter)
        │   └── RandomStrategy (time-based hash)
        └── Vec<ProviderEntry>
            └── dyn WebSearchProvider (trait_def.rs)
                ├── TavilyProvider
                ├── MiniMaxProvider
                ├── ZhiPuProvider
                ├── BochaProvider
                ├── FirecrawlProvider
                ├── AnyCrawlProvider
                ├── SerpApiProvider
                └── SerperProvider
```

### Key Components

- **ProviderLoadBalancer**: Manages providers (each with multiple API keys), handles fallback on failure
- **SelectionStrategy**: Round-robin or random provider/key selection
- **WebSearchProvider trait**: Implement this to add new providers (`src/providers/trait_def.rs`)
- **WebSearchError**: Error types with `is_auth_error()` and `is_transient()` helpers

### Request Flow
1. MCP request → `web_search` or `web_fetch` handler
2. Handler calls `ProviderLoadBalancer.search()` or `.fetch()`
3. Load balancer uses SelectionStrategy to pick provider
4. If fallback enabled, tries next provider on failure
5. Returns SearchResponse or FetchResponse

### Configuration
YAML via `config.yaml`:
- `providers[]`: List of provider configs (order = priority)
- `key_strategy`: round_robin or random for API key selection
- `provider_strategy`: round_robin or random for provider selection, with `fallback: true`

### Provider Support
- Search + Fetch: Tavily, ZhiPu, ZhiPu Coding, Firecrawl, Serper
- Search only: MiniMax, MiniMax IO, Bocha, SerpAPI
- Fetch only: AnyCrawl

Providers without fetch support are skipped automatically in `fetch()` calls.