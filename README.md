# Web Search Load Balance MCP Server

[![CI](https://github.com/quick-sort/websearch_load_balance/actions/workflows/ci.yml/badge.svg)](https://github.com/quick-sort/websearch_load_balance/actions/workflows/ci.yml)
[![Docker Pulls](https://img.shields.io/docker/pulls/quick-sort/websearch_load_balance)](https://github.com/quick-sort/websearch_load_balance/pkgs/container/websearch_load_balance)

A Rust-based MCP (Model Context Protocol) server that provides `web_search` and `web_fetch` tools with load balancing across multiple providers and API keys.

## Features

- **Multi-Provider Support**: 10+ search/fetch APIs
- **Multi-Key Load Balancing**: Round-robin or random selection across API keys within each provider
- **Provider Failover**: Automatic fallback to next provider on failure
- **YAML Configuration**: Flexible config via `config.yaml`
- **Default Base URLs**: All known provider endpoints are pre-configured
- **stdio Transport**: Works with Claude Desktop, Cursor and other MCP clients

## Supported Providers

| Provider | Code | Search | Fetch | Default Base URL |
|----------|------|--------|-------|-------------------|
| [Tavily](https://docs.tavily.com) | `tavily` | ✅ | ✅ | `https://api.tavily.com` |
| [MiniMax](https://platform.minimaxi.com) | `minimaxi` | ✅ | ❌ | `https://api.minimaxi.com` |
| [MiniMax IO](https://platform.minimaxi.com) | `minimax_io` | ✅ | ❌ | `https://api.minimaxi.io` |
| [ZhiPu](https://open.bigmodel.cn) | `zhipu` | ✅ | ✅ | `https://open.bigmodel.cn` |
| [ZhiPu Coding](https://open.bigmodel.cn) | `zhipu_coding` | ✅ | ✅ | `https://open.bigmodel.cn` |
| [Bocha](https://bochaai.com) | `bocha` | ✅ | ❌ | `https://api.bocha.cn` |
| [Firecrawl](https://firecrawl.dev) | `firecrawl` | ✅ | ✅ | `https://api.firecrawl.dev` |
| [AnyCrawl](https://anycrawl.dev) | `anycrawl` | ❌ | ✅ | `https://api.anycrawl.dev` |
| [SerpAPI](https://serpapi.com) | `serpapi` | ✅ | ❌ | `https://serpapi.com` |
| [Serper](https://serper.dev) | `serper` | ✅ | ✅ | `https://google.serper.dev` |

> For ZhiPu Coding, set `api_variant: "coding"` in settings.

## Quick Start

### 1. Install Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### 2. Build

```bash
git clone https://github.com/your-repo/websearch_load_balance.git
cd websearch_load_balance
cargo build --release
```

### 3. Configure

Edit `config.yaml` with your API keys:

```yaml
providers:
  - name: "tavily"
    enabled: true
    api_keys:
      - "tvly-your-api-key"

  - name: "minimaxi"
    enabled: true
    api_keys:
      - "your-minimax-key"
```

### 4. Run

```bash
cargo run --release
# or with custom config
cargo run --release -- --config /path/to/config.yaml
```

### 5. MCP Client Configuration

#### Claude Desktop

Add to `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "websearch": {
      "command": "/path/to/target/release/websearch_load_balance",
      "args": []
    }
  }
}
```

#### Cursor

`Preferences > MCP > Add new global MCP Server`, enter the command path.

## Configuration

### Load Balancing Strategy

```yaml
key_strategy:
  type: "round_robin"  # or "random"

provider_strategy:
  type: "round_robin"  # or "random"
  fallback: true        # failover on provider failure
```

### Provider Priority

The order in `providers` list determines priority (first = highest).

### ZhiPu Coding Endpoint

```yaml
- name: "zhipu_coding"
  enabled: true
  api_keys:
    - "your-coding-key"
  settings:
    api_variant: "coding"  # uses /api/coding/paas/v4/
```

## MCP Tools

### web_search

```json
{
  "query": "Rust programming language",
  "max_results": 5
}
```

Returns:

```json
{
  "organic": [
    {
      "title": "Rust Programming Language",
      "link": "https://www.rust-lang.org/",
      "snippet": "A language empowering everyone...",
      "date": "2024-01-01",
      "favicon": "https://example.com/favicon.ico"
    }
  ],
  "related_searches": [{"query": "Rust tutorial"}]
}
```

### web_fetch

```json
{
  "url": "https://www.rust-lang.org/"
}
```

Returns:

```json
{
  "content": "# Rust Programming Language\n\n## Why Rust?\n\n...",
  "url": "https://www.rust-lang.org/",
  "title": "Rust Programming Language"
}
```

> `web_fetch` skips providers that don't support it (MiniMax, Bocha, SerpAPI, AnyCrawl).

## Testing

```bash
cargo test           # unit tests only
cargo test -- --ignored  # integration tests (requires API keys)
```

## Docker

```bash
docker build -t websearch_load_balance .
docker run -it --rm -v $(pwd)/config.yaml:/app/config.yaml:ro websearch_load_balance
```

### Docker Compose

```bash
docker compose up -d
```

## CI/CD

GitHub Actions workflows:

| Workflow | Trigger | Purpose |
|----------|---------|---------|
| CI | push/PR (main) | Format, Clippy, Tests, Build |
| Release | tag push (v*.*.*) | Multi-platform Docker + Binary |

### GitHub Secrets

Add to `Settings > Secrets and variables > Actions`:

```
TAVILY_API_KEY, GLM_API_KEY, GLM_CODING_API_KEY, MINIMAX_API_KEY,
BOCHA_API_KEY, SERPAPI_API_KEY, SERPER_API_KEY,
FIRECRAWL_API_KEY, ANYCRAWL_API_KEY
```

### Release

```bash
git tag v0.1.0
git push origin v0.1.0
```

## Logging

```bash
RUST_LOG=debug cargo run   # verbose
RUST_LOG=info cargo run  # default
RUST_LOG=error cargo run  # errors only
```

## License

MIT