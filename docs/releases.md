# Release artifacts

Each tagged release publishes prebuilt binaries on the GitHub release page. There are
two variants; pick by whether your deployment uses the opt-in platform features.

## The two variants

| Artifact | Features | Use it when |
|----------|----------|-------------|
| `fraiseql-<target>` (**default**) | `cli,server,postgres` | You compile schemas and/or run a plain GraphQL-over-Postgres server. This is the lean binary — no V8, no extra runtime weight. |
| `fraiseql-full-<target>` | `release-full` — Deno/TypeScript functions, scheduled `sources`, `observers`, `mcp`, `inbound` + `inbound-email`, `metrics`, `federation`, and `run-server` | Your deployment uses any FraiseQL **platform feature**: `after:mutation`/`cron`/`after:capture` functions, scheduled sources, federation, the MCP endpoint, inbound ingestion, or Prometheus metrics. Contains **two** binaries — `fraiseql` and `fraiseql-server` (see below). |

The `-full` tarball contains **two** binaries:

- **`fraiseql`** — the umbrella binary (`--package fraiseql`): the compiler CLI plus
  `fraiseql run`, a development quick-launcher that compiles a schema in memory and serves
  it. `fraiseql run` reads only the `[server]` and `[database]` config sections (it warns
  and names any other section it is handed), so it is for local iteration, not production.
- **`fraiseql-server`** — the standalone production server, driven by `--config server.toml`.
  This is the entrypoint for a real deployment: it honors the full config surface
  (`[auth]`, `[federation]`, `[observers]`, `[tenancy]`, `[storage]`, `[security]`, …),
  wires the observer / tenancy / storage / token-revocation / secrets subsystems, and
  validates `sql_source`s at boot. Point a full `server.toml` at this binary — not at
  `fraiseql run`.

The lean `fraiseql-<target>` artifact ships only the umbrella `fraiseql` binary (no
`fraiseql-server`, and its `fraiseql run` is compiled out).

## Use the cli and server from the *same* release

The compiled-schema `jsonb_column` contract is a same-revision contract (#507): a schema
compiled by one revision's cli must be served by the same revision's server. Do **not**
mix a stock cli from release *N* with a server built from a different revision. The `-full`
tarball makes this contract physical: its `fraiseql` (compiler cli) and `fraiseql-server`
come from a single build of a single revision, so `fraiseql compile` and
`fraiseql-server --config server.toml` taken from the same archive are revision-matched by
construction.

## Platform matrix

The `-full` variant is built **only for native targets**, because
`functions-runtime-deno` compiles V8, which does not build under the `cross` Docker
images used for some targets.

| Target | `fraiseql-<target>` (lean) | `fraiseql-full-<target>` |
|--------|:--:|:--:|
| `x86_64-unknown-linux-gnu` | ✅ | ✅ |
| `aarch64-unknown-linux-gnu` | ✅ | ❌ — V8 does not cross-compile here |
| `x86_64-apple-darwin` | ✅ | ✅ |
| `aarch64-apple-darwin` | ✅ | ✅ |
| `x86_64-pc-windows-msvc` | ✅ | ✅ |

For **ARM Linux (`aarch64`)** with platform features, use the Docker image (built with
`fraiseql-server` + your chosen `CARGO_FEATURES`) or a from-source build on the target.
This is a documented gap, never a silent one — a native `-full` build failure fails the
release loudly.

## What "compiled in" means at runtime

A feature being in the binary does not turn it on. Every platform feature is still
opt-in at runtime configuration: Deno functions run only for the triggers your compiled
schema declares, `/mcp` mounts only when the schema declares an `mcp` block, sources run
only when declared, and metrics export only when `metrics` is configured. Compiled ≠
enabled.
