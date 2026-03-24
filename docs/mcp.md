# MCP (Model Context Protocol) Integration

FraiseQL includes built-in support for the [Model Context Protocol](https://modelcontextprotocol.io/) (MCP), an open standard that lets AI assistants and LLM-based tools interact with external systems through a uniform interface. When MCP is enabled, FraiseQL exposes its compiled GraphQL queries and mutations as MCP tools, allowing AI clients such as Claude Desktop, Claude Code, or Cursor to read and write your database using the same schema you already defined.

## Enabling MCP

MCP support is compiled behind the `mcp` Cargo feature flag. It is not included in the default feature set.

### 1. Build with the `mcp` feature

```bash
cargo build --release --features mcp
```

### 2. Enable MCP in `fraiseql.toml`

Add an `[mcp]` section to your project configuration and recompile the schema:

```toml
[mcp]
enabled = true
```

Then recompile so the MCP configuration is embedded in the compiled schema:

```bash
fraiseql-cli compile schema.json
```

### 3. Launch in stdio mode

MCP stdio mode is activated by setting the `FRAISEQL_MCP_STDIO` environment variable (any value). When this variable is present, the server reads JSON-RPC requests from **stdin** and writes responses to **stdout** instead of starting its normal HTTP listener.

```bash
FRAISEQL_MCP_STDIO=1 fraiseql-server
```

All standard server configuration still applies (database URL, schema path, etc.). The only difference is the transport layer: stdio replaces HTTP.

## Configuring AI Tools

### Claude Code

Add the following to your Claude Code MCP settings (`.mcp.json` in the project root, or `~/.claude/mcp.json` globally):

```json
{
  "mcpServers": {
    "fraiseql": {
      "command": "fraiseql-server",
      "env": {
        "FRAISEQL_MCP_STDIO": "1",
        "DATABASE_URL": "postgres://user:pass@localhost:5432/mydb",
        "FRAISEQL_SCHEMA_PATH": "./schema.compiled.json"
      }
    }
  }
}
```

### Cursor

In Cursor settings, add an MCP server entry:

```json
{
  "mcpServers": {
    "fraiseql": {
      "command": "fraiseql-server",
      "env": {
        "FRAISEQL_MCP_STDIO": "1",
        "DATABASE_URL": "postgres://user:pass@localhost:5432/mydb",
        "FRAISEQL_SCHEMA_PATH": "./schema.compiled.json"
      }
    }
  }
}
```

### Claude Desktop

In Claude Desktop's `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "fraiseql": {
      "command": "fraiseql-server",
      "env": {
        "FRAISEQL_MCP_STDIO": "1",
        "DATABASE_URL": "postgres://user:pass@localhost:5432/mydb",
        "FRAISEQL_SCHEMA_PATH": "./schema.compiled.json"
      }
    }
  }
}
```

## Available Tools

Every query and mutation in your compiled schema is automatically exposed as an MCP tool. The tool name matches the GraphQL operation name, and the input schema is derived from the operation's argument definitions.

For example, given this schema:

```python
@fraiseql.type
class User:
    id: int
    name: str
    email: str

@fraiseql.query(returns=User)
def users(limit: int = 10): ...

@fraiseql.mutation(sql_source="create_user", operation="INSERT")
def createUser(name: str, email: str): ...
```

The following MCP tools are registered:

| Tool name | Type | Arguments | Description |
|-----------|------|-----------|-------------|
| `users` | Query | `limit` (integer, optional) | Fetches user records |
| `createUser` | Mutation | `name` (string, required), `email` (string, required) | Creates a new user |

When an AI client calls a tool, FraiseQL builds a GraphQL query from the tool name and arguments, executes it through the standard query pipeline (including RLS, caching, and validation), and returns the JSON result.

### How tool calls work internally

1. The MCP client sends a `tools/call` JSON-RPC request with the tool name and arguments.
2. FraiseQL constructs a GraphQL operation: `query { users(limit: 10) { id name email } }`.
3. The operation is executed through the existing `Executor`, applying all security rules.
4. The JSON result is returned as an MCP text content block.

## Configuration Reference

All MCP settings live under `[mcp]` in `fraiseql.toml`:

```toml
[mcp]
# Enable MCP server endpoint (default: false).
enabled = true

# Transport mode: "http", "stdio", or "both" (default: "http").
# When using FRAISEQL_MCP_STDIO=1, the transport setting is overridden to stdio.
transport = "stdio"

# HTTP path for the MCP endpoint, used when transport includes "http" (default: "/mcp").
path = "/mcp"

# Require authentication for MCP requests (default: true).
require_auth = true

# Whitelist of query/mutation names to expose as tools (default: [] = all).
# When non-empty, only the listed operations are registered as MCP tools.
include = ["users", "getUserById"]

# Blacklist of query/mutation names to hide (default: []).
# These operations are never exposed, even if they match the include list.
exclude = ["deleteAllUsers", "dangerousReset"]
```

### Filtering exposed tools

Use `include` and `exclude` to control which operations appear as MCP tools:

- **Both empty** (default): all queries and mutations are exposed.
- **`include` non-empty**: only the listed operations are exposed.
- **`exclude` non-empty**: the listed operations are hidden; everything else is exposed.
- **Both non-empty**: an operation must be in `include` AND not in `exclude` to be exposed.

This is useful for hiding administrative mutations from AI clients while still exposing read queries.

## Limitations

- **Stdio transport only.** The current implementation supports the MCP stdio transport. The `FRAISEQL_MCP_STDIO` environment variable switches the server from HTTP to stdio mode; they cannot run simultaneously in a single process.
- **Read-only by default.** While mutations are technically available as MCP tools, it is strongly recommended to use `exclude` to hide write operations unless you have a specific use case and appropriate access controls in place. RLS and authentication still apply, but AI-initiated writes carry inherent risk.
- **No streaming.** MCP tool results are returned as a single JSON text block. Large result sets should be paginated using query arguments (`limit`/`offset`).
- **Feature flag required.** MCP support is not compiled by default. You must build with `--features mcp` to include it. This keeps the binary size minimal for deployments that do not need MCP.
- **Single session.** The stdio transport serves one MCP client at a time. For multi-client scenarios, run separate server processes.
