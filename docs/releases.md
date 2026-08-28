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
  (`[auth]`, `[observers]`, `[tenancy]`, `[storage]`, `[rate_limiting]`, `[tls]`, …),
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

The `-full` variant is built for every target whose toolchain can link V8, which
`functions-runtime-deno` compiles in. That is now every released target except musl.

| Target | `fraiseql-<target>` (lean) | `fraiseql-full-<target>` |
|--------|:--:|:--:|
| `x86_64-unknown-linux-gnu` | ✅ | ✅ |
| `aarch64-unknown-linux-gnu` | ✅ | ✅ |
| `x86_64-unknown-linux-musl` | ✅ | ❌ — V8 does not link against musl |
| `x86_64-apple-darwin` | ✅ | ✅ |
| `aarch64-apple-darwin` | ✅ | ✅ |
| `x86_64-pc-windows-msvc` | ✅ | ✅ |

`aarch64-unknown-linux-gnu` shipped lean-only until #649. It was cross-compiled from
x86 with `cross`, and V8 does not build under those Docker images — so ARM-Linux
adopters got neither the platform-feature umbrella nor a prebuilt `fraiseql-server`.
It is built on a native arm64 runner now, which removes the constraint rather than
working around it, and it carries the same glibc floor (2.28 via `cargo-zigbuild`)
and ceiling gate (2.34) as every other Linux artifact.

The artifact is **booted on real arm64 before it ships**, not merely linked: the
`arm64-full` job in `release-smoke.yml` builds it on an arm64 runner, checks the glibc
ceiling, confirms the binaries really are aarch64, runs `functions invoke --help` to
prove V8 initialises, then starts `fraiseql-server` against a real PostgreSQL and
serves a GraphQL query. That job is dispatchable, so the arm64 path can be exercised
without cutting a release — the alternative is discovering a V8-versus-glibc-floor
disagreement in the middle of one.

For **ARM Linux with musl** (Alpine, distroless, scratch), use the lean static binary
or the Docker image with your chosen `CARGO_FEATURES`.

## What "compiled in" means at runtime

A feature being in the binary does not turn it on. Every platform feature is still
opt-in at runtime configuration: Deno functions run only for the triggers your compiled
schema declares, `/mcp` mounts only when the schema declares an `mcp` block, sources run
only when declared, and metrics export only when `metrics` is configured. Compiled ≠
enabled.

## Which artifacts CI actually verifies

An artifact can be published and never once have been run. This section says which of the
things this project ships are proved to work by a CI leg, and which are only proved to
exist — because an unstated gap is worse than a known one.

The machine-readable ledger is [`tools/delivery-artifacts.toml`](../tools/delivery-artifacts.toml),
checked on every push by `tools/check-delivery-coverage.py`. That gate makes this section
hard to let rot: a new shipped artifact cannot arrive without a row, a row cannot outlive
its artifact, and an artifact with no executing leg must carry an exemption naming the
issue that owns the gap.

**Executed — a leg runs it and requires an answer only a working artifact can give:**

| Artifact | What proves it |
|---|---|
| `fraiseql-server` image | Boots on its own `CMD` against a real Postgres, answers a GraphQL query resolved through SQL, then returns a row inserted *after* it was already serving. Its linkage, uid, labels, `EXPOSE` and version are read off the built image, and its `HEALTHCHECK` is executed in three states. |
| `fraiseql-server-full` image | Same boot and property tiers as above. |
| Helm chart | Deployed into a throwaway k3s cluster on that image, queried through its Service, then re-queried after a row is inserted behind it. |
| Compose stack (root `docker-compose.yml`) | Brought up on that image, becomes healthy on the image's *own* healthcheck, answers through its published port, then returns a row inserted after it was serving. |
| `fraiseql-full-x86_64-unknown-linux-gnu.tar.gz` | Downloaded from the release, extracted, and the shipped `fraiseql-server` booted via `--config` against a real Postgres, asserting `/health` and a token-gated `/metrics`. |

All but the last run on the `Dagger — image` leg, on every push to `dev` and `release/*` —
that is, **before** the tag.

**Not executed — published and checked for presence only:**

| Artifact | What is actually checked | Issue |
|---|---|---|
| 18 crates on crates.io | An HTTP-200 probe per crate after publish. Nothing installs or compiles one from the registry. | [#1222](https://github.com/fraiseql/fraiseql/issues/1222) |
| `fraiseql` on PyPI and npm | The same HTTP-200 probe. The SDK conformance suite runs the in-tree source, never the published package. | [#1222](https://github.com/fraiseql/fraiseql/issues/1222) |
| The other 10 release binaries | Asserted present on the release **by name**. Never downloaded or run. | [#1222](https://github.com/fraiseql/fraiseql/issues/1222) |
| `tutorial` image | Built before the tag; nothing starts it. | [#1221](https://github.com/fraiseql/fraiseql/issues/1221) |

**Not published at all.** Nine of the eleven official schema-authoring SDKs are published
to no registry at any version — only Python (PyPI) and TypeScript (npm) are. Seven of the
nine have a working publish job gated on a `<sdk>/v*` tag, and no such tag has ever been
cut; see [#1130](https://github.com/fraiseql/fraiseql/issues/1130). Two have defects of
their own: the Dart job's only publish step is a `--dry-run`
([#1223](https://github.com/fraiseql/fraiseql/issues/1223)), and the Go SDK's declared
module path names a repository that does not exist
([#1224](https://github.com/fraiseql/fraiseql/issues/1224)).
