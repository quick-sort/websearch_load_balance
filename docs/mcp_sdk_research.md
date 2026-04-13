# Rust MCP SDK Research Report

This document provides a comprehensive guide to building MCP (Model Context Protocol) servers in Rust using the official Rust SDK.

## Table of Contents

1. [Main Crate for Building MCP Servers](#1-main-crate-for-building-mcp-servers)
2. [Creating a Simple MCP Server with Tools](#2-creating-a-simple-mcp-server-that-exposes-tools)
3. [Tool Input Schemas](#3-tool-input-schemas)
4. [Handling Tool Invocations](#4-handling-tool-invocations)
5. [Stdio Transport](#5-stdio-transport)

---

## 1. Main Crate for Building MCP Servers

The main crate for building MCP servers in Rust is **`rmcp`** (also written as "RMCP" or "rust-mcp-sdk").

### Crates.io Information

| Crate | Downloads | Repository |
|-------|-----------|------------|
| `rmcp` | 157,424+ | [modelcontextprotocol/rust-sdk](https://github.com/modelcontextprotocol/rust-sdk) |
| `rust-mcp-sdk` | 105,238+ | [rust-mcp-stack/rust-mcp-sdk](https://github.com/rust-mcp-stack/rust-mcp-sdk) |

The official `rmcp` crate is maintained by the Model Context Protocol team and is the recommended choice.

### GitHub Repository

- **Official**: https://github.com/modelcontextprotocol/rust-sdk
- **Alternative**: https://github.com/4t145/rmcp (a cleaner implementation)

---

## 2. Creating a Simple MCP Server that Exposes Tools

### Cargo.toml Dependencies

```toml
[dependencies]
tokio = { version = "1", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
schemars = { version = "0.8", features = ["derive"] }
rmcp = { version = "0.8", features = ["server", "macros", "schemars", "transport-io"] }
```

### Key Features

| Feature | Description |
|---------|-------------|
| `server` | Server-side implementation |
| `macros` | Procedural macros (`#[tool]`, `#[tool_router]`, `#[tool_handler]`) |
| `schemars` | Automatic JSON Schema generation |
| `transport-io` | Stdio transport for servers |

### Complete Example

```rust
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, Content, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router,
    transport::stdio, ErrorData as McpError, ServiceExt, Json,
};
use std::sync::Arc;
use tokio::sync::Mutex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// Define input struct that derives JsonSchema for automatic schema generation
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct CalculationRequest {
    pub a: i32,
    pub b: i32,
}

// Define output struct
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct CalculationResult {
    pub sum: i32,
    pub product: i32,
}

#[derive(Clone)]
pub struct MyServer {
    counter: Arc<Mutex<i32>>,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl MyServer {
    fn new() -> Self {
        Self {
            counter: Arc::new(Mutex::new(0)),
            tool_router: Self::tool_router(),
        }
    }

    #[tool(description = "Increment the counter by 1")]
    async fn increment(&self) -> Result<CallToolResult, McpError> {
        let mut counter = self.counter.lock().await;
        *counter += 1;
        Ok(CallToolResult::success(vec![Content::text(
            counter.to_string(),
        )]))
    }

    #[tool(description = "Get the current counter value")]
    async fn get_counter(&self) -> Result<CallToolResult, McpError> {
        let counter = self.counter.lock().await;
        Ok(CallToolResult::success(vec![Content::text(
            counter.to_string(),
        )]))
    }

    #[tool(name = "calculate", description = "Perform calculations on two numbers")]
    pub async fn calculate(
        &self,
        params: Parameters<CalculationRequest>,
    ) -> Result<Json<CalculationResult>, McpError> {
        Ok(Json(CalculationResult {
            sum: params.0.a + params.0.b,
            product: params.0.a * params.0.b,
        }))
    }
}

#[tool_handler]
impl rmcp::ServerHandler for MyServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some("A simple counter and calculator server.".into()),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create and run the server with STDIO transport
    let service = MyServer::new().serve(stdio()).await.inspect_err(|e| {
        eprintln!("Error starting server: {}", e);
    })?;
    service.waiting().await?;

    Ok(())
}
```

---

## 3. Tool Input Schemas

### Schema Definition with schemars

The `input_schema` and `output_schema` fields are automatically generated using the `schemars` crate.

- **Input Schema**: If a function parameter is wrapped in `Parameters<T>`, the macro generates the input schema for `T`. If no `Parameters<T>` is found, it defaults to an empty object schema.
- **Output Schema**: If the return type is `Json<R>` or `Result<Json<R>, E>`, the macro automatically generates the output schema for `R`.

### Tool Struct Definition

The core structure for a tool is `rmcp::model::Tool`:

```rust
pub struct Tool {
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub input_schema: Arc<JsonObject>,  // JSON Schema for input
    pub output_schema: Option<Arc<JsonObject>>, // JSON Schema for output
    pub annotations: Option<JsonObject>,
    pub icons: Option<Vec<Icon>>,
    pub meta: Option<JsonObject>,
}
```

### Manual Schema Definition

You can also define a tool manually with custom input schema:

```rust
use std::sync::Arc;
use serde_json::json;

fn make_manual_tool() -> Tool {
    let input_schema = Arc::new(
        json!({
            "type": "object",
            "properties": {
                "email": {
                    "type": "string",
                    "format": "email",
                    "description": "Recipient address"
                },
                "subject": {
                    "type": "string",
                    "minLength": 1,
                    "maxLength": 200,
                    "description": "Subject line"
                },
                "body": {
                    "type": "string",
                    "description": "Message body"
                }
            },
            "required": ["email", "subject", "body"],
            "additionalProperties": false
        })
        .as_object()
        .unwrap()
        .clone(),
    );

    Tool::new(
        "send_email_manual",
        Some("Send an email (manual schema)"),
        input_schema,
    )
}
```

### Using Parameters Wrapper

The `Parameters<T>` wrapper type is used to extract tool input from the request context:

```rust
#[tool(description = "Calculate the sum of two numbers")]
async fn sum(
    &self,
    Parameters(StructRequest { a, b }): Parameters<StructRequest>,
) -> Result<CallToolResult, McpError> {
    Ok(CallToolResult::success(vec![Content::text((a + b).to_string())]))
}
```

---

## 4. Handling Tool Invocations

### call_tool Mechanism

The `call_tool` mechanism involves routing incoming tool call requests to the appropriate tool handler.

1. The `#[tool_handler]` macro implements the `ServerHandler` trait for your type
2. It automatically generates the `call_tool` and `list_tools` methods
3. The `#[tool_router]` macro generates a `ToolRouter<Self>` that collects all tools
4. The `ToolRouter` maps tool names to `ToolRoute` instances containing metadata and handlers

### Tool Call Flow

```
Client Request (JSON-RPC)
    → Server.serve(transport)
    → ToolRouter.call(ToolCallContext)
    → #[tool] annotated function
    → CallToolResult
    → Response to client
```

### Macros Used

| Macro | Purpose |
|-------|--------|
| `#[tool]` | Marks a function as a tool |
| `#[tool_router]` | Generates ToolRouter for impl block |
| `#[tool_handler]` | Implements ServerHandler trait |

### Tool Invocation Example

```rust
#[tool(description = "Say hello to the client")]
fn say_hello(&self) -> Result<CallToolResult, McpError> {
    Ok(CallToolResult::success(vec![Content::text("hello")]))
}
```

The return type `CallToolResult` is constructed using helper methods:

```rust
// Success
Ok(CallToolResult::success(vec![Content::text("result")]))

// Failure
Err(McpError::tool_not_found("tool_name", Some(json!({"reason": "not found"})))
```

---

## 5. Stdio Transport

The stdio transport allows an MCP server to communicate via standard input and output streams. This is typically used when an external process launches the MCP server.

### Transport Setup

```rust
use rmcp::{ServiceExt, transport::stdio};
use tokio::io::{stdin, stdout};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let handler = MyServer::new();
    
    // Create stdio transport
    let transport = stdio();
    
    // Serve on stdio
    let server = handler.serve(transport).await?;
    
    // Wait for completion
    server.waiting().await?;
    Ok(())
}
```

### Alternative Tuple Syntax

```rust
let server = handler.serve((stdin(), stdout())).await?;
```

### JSON-RPC Communication Flow

1. Incoming JSON-RPC requests are read from `tokio::io::Stdin`
2. Parsed by an `AsyncRW` Codec
3. Routed to the appropriate `ServerHandler`
4. Responses are serialized and written to `tokio::io::Stdout`

### TokioChildProcess for Clients

To spawn an MCP server as a child process from a client:

```rust
use rmcp::{ServiceExt, transport::{TokioChildProcess, ConfigureCommandExt}};
use tokio::process::Command;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = ().serve(
        TokioChildProcess::new(
            Command::new("npx").configure(|cmd| { 
                cmd.arg("-y").arg("@modelcontextprotocol/server-everything"); 
            })
        )?
    ).await?;
    
    // Use client to interact with server
    let tools = client.list_tools(Default::default()).await?;
    Ok(())
}
```

---

## Additional Resources

### Related Projects Built with rmcp

- [rustfs-mcp](https://github.com/rustfs/rustfs/tree/main/crates/mcp) - S3-compatible object storage operations
- [containerd-mcp-server](https://github.com/jokemanfire/mcp-containerd) - containerd-based MCP server
- [rmcp-openapi-server](https://gitlab.com/lx-industries/rmcp-openapi/-/tree/main/crates/rmcp-openapi-server) - OpenAPI to MCP tools
- [nvim-mcp](https://github.com/linw1995/nvim-mcp) - Neovim integration

### MCP Specification

- [Official MCP Specification](https://spec.modelcontextprotocol.io/specification/2024-11-05/)
- [Schema Definition](https://github.com/modelcontextprotocol/specification/blob/main/schema/2024-11-05/schema.ts)

---

## Quick Reference

### Cargo.toml

```toml
[dependencies]
rmcp = { version = "0.8", features = ["server", "macros", "schemars", "transport-io"] }
tokio = { version = "1", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
schemars = { version = "0.8", features = ["derive"] }
```

### Minimum Server Code

```rust
use rmcp::{tool, tool_handler, tool_router, ServiceExt, transport::stdio};

#[tool_router]
impl MyServer {
    #[tool(description = "A simple tool")]
    async fn my_tool(&self) -> Result<CallToolResult, McpError> {
        Ok(CallToolResult::success(vec![Content::text(" résultat")]))
    }
}

#[tool_handler]
impl rmcp::ServerHandler for MyServer {}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    MyServer.serve(stdio()).await?.waiting().await?;
    Ok(())
}
```