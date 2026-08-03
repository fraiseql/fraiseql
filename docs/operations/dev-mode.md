# Dev Mode: Iterating with `fraiseql watch`

FraiseQL's edit loop is compile-based: change the schema source, produce a new
`schema.compiled.json`, and get it into a running server. Two commands automate that loop;
they differ in **who owns the server process**.

| Command | Server process | Reload mechanism | Use when |
|---------|----------------|------------------|----------|
| `fraiseql run --watch` | Owned by the command — recompiles and restarts in-process on every save | Process restart | You want one terminal doing everything |
| `fraiseql watch` | A **separately running** `fraiseql-server` you started yourself | `POST /api/v1/admin/reload-schema` → `ArcSwap` swap, zero downtime | The server has its own lifecycle (docker compose, a debugger, a shared dev box) |

## `fraiseql watch`

```bash
fraiseql watch schema.json \
  --reload-url http://localhost:8080 \
  --admin-token "$FRAISEQL_ADMIN_TOKEN"
```

On start it compiles once (so the loop begins from a known-good state), then watches the
input and repeats on every save:

1. **Debounce.** Rapid successive events — the double-write most editors emit on an atomic
   save — collapse into one compile (300 ms quiet window).
2. **Compile.** `schema.json` (or `fraiseql.toml`) → `schema.compiled.json`. Pass
   `--database <url>` to run the schema↔database drift linter on every save; since #384,
   error-severity drift (a `sql_source` that does not exist, a required field's JSONB key
   missing from sampled rows, a mutation with no backing function) **fails the compile**.
3. **Reload** (only with `--reload-url`). POSTs the compiled path to the server's
   `POST /api/v1/admin/reload-schema` admin endpoint. The server builds the new executor
   and swaps it via `ArcSwap`: in-flight queries finish on the old schema, new requests see
   the new one — no restart, no dropped connections (the same mechanism as
   [zero-downtime deploys](zero-downtime-deploys.md)).

### Failure semantics

A broken save must never take down your dev loop or your dev server:

- **Compile failures are reported and skipped.** The previous good
  `schema.compiled.json` stays on disk, the server keeps serving it, and the loop keeps
  watching — fix the source and save again. (Pinned by `watch_loop_test.rs`: a broken save
  leaves the artifact byte-identical.)
- **Reload failures are reported and skipped** the same way (server down, wrong token).

### Access control

The reload endpoint is part of the admin write surface and requires the admin **bearer
token** — it is authenticated, not environment-gated, so the same binary is safe in every
environment: without an admin token configured, the admin surface is not mounted at all.
Use `--admin-token` (or omit `--reload-url` entirely and let a file-watching supervisor
restart the server instead).

## Compile-error feedback

Compile errors print with the failing file and reason, prefixed `[watch]`, and — with
`--database` — the full drift report (every finding, not just the first). See
`fraiseql doctor --against-db <url> --json` for the same drift report in a
machine-readable form.
