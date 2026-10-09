# Session variables (`[session_variables]`)

Session variables give your SQL per-request values. Before each statement, FraiseQL sets
each declared variable transaction-locally (`set_config(name, value, true)`), so views,
functions and row-level security policies read it with `current_setting(name, true)`.

```toml
# fraiseql.toml
[[session_variables.variables]]
name   = "app.tenant_id"
source = "jwt"
claim  = "tenant_id"

[[session_variables.variables]]
name   = "app.region"
source = "header"
header = "x-region"

[[session_variables.variables]]
name   = "app.flavor"
source = "literal"
value  = "vanilla"
```

## Sources

| `source` | Value | Without a principal |
|---|---|---|
| `jwt` | The named claim of the verified token. `sub` falls back to the user id and `tenant_id` to the tenant. A missing claim leaves the variable unset. | unset |
| `enrichment` | A field of the [enriched identity](../architecture/enriched-identity-rls.md), with no fallback. A missing field is an error. | unset |
| `header` | The named request header (gRPC: request metadata), matched case-insensitively. An absent header leaves the variable unset. | set |
| `literal` | The fixed `value`. | set |

An anonymous request gets its `header` and `literal` variables. `jwt` and `enrichment`
variables need a principal, so they stay unset.

## A header is client-controlled

Any client can send any header with any value. **Never use a `header` variable for
row-level security or tenant scoping.** Scope rows with a `jwt` or `enrichment` variable,
whose value comes from a verified token or from your database.

`fraiseql doctor --against-db` warns for each RLS policy whose `USING` or `WITH CHECK`
expression calls `current_setting` on a header-sourced variable. It reads the policy text,
so it does not see a policy that reads the variable through a function it calls.

A header-sourced value is never read from a claim of the same name: the header and the
token's claims are different inputs. A request is refused, with a validation error, when a
header a variable reads:

- was sent more than once (values are never joined);
- is not valid UTF-8;
- is longer than 1024 bytes (values are never truncated).

## Transports

Each transport reads the headers once per request and runs the request in them: GraphQL
over HTTP (including SSE `@stream` continuation batches), REST, MCP, gRPC and async
operations, which store the headers at submission so the worker that executes the
operation later runs with them. Transports without request headers set no `header`
variables: the MCP stdio transport and Arrow Flight. The admin SQL console's RLS preview
has no request either, so its preview sets none. Subscriptions over `/ws` run no SQL per
event, so no session variable reaches them.

## Reads and writes

Reads and writes both carry the variables. A write also carries `fraiseql.started_at`
([mutation timing](mutation-timing.md)); a read also carries `fraiseql.locale`
([request locale](request-locale.md)); declaring a variable named `fraiseql.locale` is
refused.
