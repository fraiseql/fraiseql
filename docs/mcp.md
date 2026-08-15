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
2. FraiseQL resolves the tool name against the advertised tool list — the same list `tools/list` returns, after `include`/`exclude`/`read_only`. A name that is not on it is refused here.
3. FraiseQL constructs a GraphQL operation whose values travel as variables: `query ($limit: Int) { users(limit: $limit) { id name email } }` with `{"limit": 10}`. Argument *names* must be ones the operation declares; anything else is refused.
4. The caller's tenant is resolved (JWT claim, `X-Tenant-ID`, or `Host`) and the operation is executed on that tenant's `Executor`, applying all security rules.
5. The JSON result is returned as an MCP text content block. Execution errors pass through the configured error sanitizer first.

## Resources and Prompts

Alongside tools, the server advertises two discovery surfaces (#967). Both are
derived from the **same** exposed set `tools/list` uses, so `include`, `exclude`
and `read_only` govern all three at once — an operation you withheld is not
advertised, described, or readable.

### Resources — `fraiseql://query/{name}`

Every exposed **query** is a readable Resource. Mutations are not: `resources/read`
is a read verb in every MCP client, and a mutation behind it would be a write
under a verb that promises otherwise.

```
resources/list  → [{ "uri": "fraiseql://query/users", "name": "users",
                     "description": "Fetches user records",
                     "mimeType": "application/json" }, …]
resources/read  → { "contents": [{ "uri": "…", "text": "<the query's JSON result>" }] }
```

**Reading a Resource runs the same operation, through the same path, as calling
its tool.** `read_resource` delegates to the tool seam rather than executing on
its own, so authentication, tenant resolution, the concurrency and per-second
quotas, RLS, `requires_role`, `requires_actor` and the field gates are the tool
path's — not a second copy that could drift. There is one execution path, which
is why RLS parity here is structural rather than asserted.

A refused read is a **protocol error**, never a successful read whose body says
"access denied": `resources/read` has no error flag, so returning the refusal as
content would hand a client a success it did not get.

Queries whose return type declares a vector field additionally publish a
`similarity-search` **resource template**, so a RAG client can discover that the
operation takes an embedding rather than inferring it from the tool's JSON
schema. The template is a discovery aid — its URI resolves through the same
`read_resource`.

### Prompts

Every exposed operation — queries **and** mutations — is advertised as a Prompt:
the operation's own `description` rendered as an instruction, with one argument
per operation argument and required-ness taken from nullability.

```
prompts/list → [{ "name": "createUser", "description": "Creates a new user",
                  "arguments": [{ "name": "name", "required": true }, …] }, …]
prompts/get  → the instruction, with the supplied arguments substituted
```

Mutations are included here even though they are excluded from Resources, because
a prompt is a sentence and getting one changes nothing. Whether the agent may then
*call* the operation is the tool allowlist's decision — and if you set
`read_only = true`, the mutation is not in the exposed set at all, so it is not
described either.

`prompts/get` touches no database and needs no identity.

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

# Never expose any mutation as a tool, whatever `include` says (default: false).
# See "Prefer read_only = true" under Limitations.
read_only = true
```

### Filtering exposed tools

Use `include` and `exclude` to control which operations are reachable as MCP tools:

- **Both empty** (default): all queries and mutations are exposed.
- **`include` non-empty**: only the listed operations are exposed.
- **`exclude` non-empty**: the listed operations are hidden; everything else is exposed.
- **Both non-empty**: an operation must be in `include` AND not in `exclude` to be exposed.

The names in these lists are the names tools are **advertised** under, which under
`naming_convention = "camelCase"` (the compiler default) are the camelCased ones —
`listUsers`, not `list_users`. The raw compiled name is not an alias and does not
reach the operation.

Filtering is enforced where the call executes, not only where the tool list is
built: naming a withheld operation directly in `tools/call` is refused, and the
refusal is the same "unknown tool" answer a genuinely nonexistent name gets, so it
cannot be used to probe for hidden operations.

This is useful for hiding administrative mutations from AI clients while still exposing read queries.

### Authentication

MCP accepts the same two Bearer-token modes as `/graphql`: OIDC (`[auth]`) or
local HS256 (`[auth_hs256]`). With `require_auth = true` (the default) and
neither configured, the HTTP endpoint refuses to mount, loudly. A validated
token becomes the same security context `/graphql` builds — the JWT's `org_id`
resolves the tenant, custom claims feed RLS session variables, and the #390
actor classification is derived — so an MCP call is authorized exactly like a
GraphQL request, by construction.

### Behaviour hints (tool annotations)

Every advertised tool carries MCP `annotations` so agent clients can behave
appropriately without guessing:

- **Queries**: `readOnlyHint: true`, `openWorldHint: false`.
- **Mutations**: `readOnlyHint: false`, `destructiveHint: true`,
  `idempotentHint: false` — deliberately conservative (the schema cannot prove
  a backing function is additive-only or idempotent), so a well-behaved client
  asks for confirmation before invoking a write.

### Audit trail

An MCP-originated mutation's change-log row (`core.tb_entity_change_log`) is
tagged `extra_metadata.transport = "mcp"`, alongside the #390 actor columns —
so "everything AI agents wrote through MCP" is one query:

```sql
SELECT created_at, object_type, modification_type, object_id, actor_type
FROM core.v_entity_change_log
WHERE extra_metadata->>'transport' = 'mcp'
ORDER BY created_at DESC;
```

The tag is stamped server-side from the transport itself (a framework-reserved
security-context attribute the extractor strips from token claims), so a caller
cannot forge or suppress it.

### Tenancy and errors

An MCP tool call goes through the same per-tenant dispatch as `/graphql`: the tenant
key is resolved from the validated token's claims (`tenant_id`, or `org_id`), the
`X-Tenant-ID` header, or the `Host` header; an unregistered key is refused rather
than served from the default executor, a suspended tenant is refused, and the
tenant's concurrency and per-second quotas apply. The stdio transport carries no
headers, so there the JWT claim is the only source.

Execution errors are passed through the configured
`[security.error_sanitization]` sanitizer before reaching the client, so a database
fault does not hand an AI agent internal relation names or SQLSTATE codes.

## Limitations

- **One transport per process.** `transport` selects `"http"` (default), `"stdio"`, or `"both"`; setting `FRAISEQL_MCP_STDIO=1` switches the process to stdio. HTTP and stdio cannot serve simultaneously in a single process.
- **Prefer `read_only = true`.** Set `[mcp] read_only = true` unless you deliberately
  expose write operations — it guarantees **no mutation is ever exposed as an MCP tool**,
  regardless of `include`/`exclude`, and (critically) a mutation added to the schema later
  is not silently exposed to AI callers. `read_only` wins over `include` (fail-closed;
  a load-time warning is logged if `include` names a mutation under `read_only`). Using
  `exclude` alone is a footgun: every future write op must be remembered, and a forgotten
  one is exposed. RLS and authentication still apply, but AI-initiated writes carry
  inherent risk, so default to read-only.
- **No streaming.** MCP tool results are returned as a single JSON text block. Large result sets should be paginated using query arguments (`limit`/`offset`).
- **Feature flag required.** MCP support is not compiled by default. You must build with `--features mcp` to include it. This keeps the binary size minimal for deployments that do not need MCP.
- **Single session.** The stdio transport serves one MCP client at a time. For multi-client scenarios, run separate server processes.
- **No cross-call session state yet.** Each tool call is independent; the server keeps no per-thread working memory for an agent. Binding the `[session_state]` store into the MCP transport is tracked at [#967](https://github.com/fraiseql/fraiseql/issues/967).
- **Tenant cost budgets do not apply to MCP.** An MCP document's shape is fixed by the schema, so its estimated cost is constant per tool and the check would meter nothing — it would either always pass or permanently disable that tool. Volume is bounded by the tenant's concurrency permit and per-second limiter, which do apply, and the schema-wide `[security.cost_budget] per_request_max` is enforced inside the executor for MCP as for every transport.
