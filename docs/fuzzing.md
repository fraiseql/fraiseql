# Fuzzing Guide

FraiseQL uses [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz) (libFuzzer) to find panics, correctness bugs, and potential security issues in parsing and code generation.

## Prerequisites

```bash
# Install nightly toolchain (required by cargo-fuzz). CI pins a specific nightly
# — see FUZZ_NIGHTLY in .github/workflows/fuzz.yml — so use the same one when
# reproducing a CI finding.
rustup toolchain install nightly

# Install cargo-fuzz
cargo install cargo-fuzz
```

## Fuzz Targets

25 targets exist across 8 crates. Thirteen of them run in the scheduled campaign
(marked **scheduled** below); the rest are runnable on demand. A target is only
added to the schedule once it is build-verified against the current API — see
"Adding a New Fuzz Target".

**Compiling is a merge gate; finding a crash is not.** Each `fuzz/` directory is
its own cargo workspace, so `cargo check --workspace`, `clippy` and every test leg
skip them entirely — and for a long time nothing else compiled them either. A
target that stopped compiling was silently absent from the campaign until someone
ran the loop below: the PostgreSQL-only de-scope (#374) broke two `fraiseql-db`
targets that way, and #1198's signature change broke the same two again for ten
days and two red scheduled runs (#1254).

`make check-fuzz` (`tools/check-fuzz-compiles.sh`) now `cargo check`s every fuzz
manifest, and runs in `make preflight` and in the Dagger `preflight` leg beside
clippy and check-default. It also asserts the other direction — every target file
on disk is a `[[bin]]` — because `cargo check` only builds declared bins, so an
undeclared target would be skipped in silence. `connection_string.rs` sat in
exactly that state in `fraiseql-wire` from the day it was written until it was
deleted; see "Properties that live as in-crate proptests" for why it was never a
fuzz target.

This does not make the *campaign* a merge gate — see "It is an assurance signal,
not a merge gate". Whether a target compiles is deterministic; whether it finds a
crash is not, and only the first is gated.

### fraiseql-core (8 targets)

| Target | What It Fuzzes | Correctness Checks |
|--------|---------------|-------------------|
| `graphql_parser` **scheduled** | GraphQL query parsing | JSON roundtrip, error quality |
| `complexity` **scheduled** | Depth/complexity/alias validation | Never panics; carries the #976 regression |
| `value_json_seam` **scheduled** | Inline argument write→read round trip | #719 — an argument never silently vanishes |
| `schema_deser` **scheduled** | Schema JSON deserialization | Roundtrip + structural equality |
| `sql_codegen` | WHERE clause → SQL generation | Balanced parens/quotes |
| `schema_compile` | Schema compilation pipeline | Roundtrip, name validation |
| `query_variables` | Query variable definitions | Name/type invariants |
| `rate_limiter` | Rate-limiter key handling | Never panics |

### fraiseql-wire (3 targets)

| Target | What It Fuzzes | Correctness Checks |
|--------|---------------|-------------------|
| `protocol_decode` **scheduled** | PostgreSQL wire protocol | Consumed bytes bounds |
| `json_validate` **scheduled** | JSONB row parsing | No panic, no unbounded recursion |
| `scram_parse` | SCRAM-SHA-256 messages | RFC 5802 format, error quality |

### fraiseql-db (4 targets)

| Target | What It Fuzzes | Correctness Checks |
|--------|---------------|-------------------|
| `where_from_json` **scheduled** | JSON → WHERE clause | Never panics, typed and untyped |
| `where_generator` **scheduled** | WHERE SQL generation | #833 — quotes and parens stay balanced |
| `identifier_validation` **scheduled** | The `where`/`orderBy` name boundary | #794/#795/#833 — accepted names cannot alter SQL |
| `projection_generator` **scheduled** | Projection SQL generation | Never panics |

### Other crates

| Crate | Targets |
|-------|---------|
| `fraiseql-federation` | `subgraph_response` **scheduled** |
| `fraiseql-server` | `graphql_request` **scheduled**, `toml_config` **scheduled** |
| `fraiseql-auth` | `jwt_parse`, `pkce_token_parse`, `state_token_decrypt` |
| `fraiseql-secrets` | `encrypted_field_decode`, `vault_secret_name` |
| `fraiseql-arrow` | `db_convert`, `flight_ticket` |

### Properties that live as in-crate proptests

Two of the properties derived from real defects are checked by `proptest` inside
the crate rather than by a libFuzzer target, because the code they guard is behind
a private module:

| Property | Where | Defect |
|---|---|---|
| A query parameter never bleeds into the host, user or database | `fraiseql-wire`, `client::connection_string::tests` | #817 |
| No caller-supplied argument value can add a root field to a built MCP document | `fraiseql-server`, `mcp::tests::executor_tests` | #808 |

Making `connection_string` or the MCP executor `pub` purely so a fuzz target could
reach them would enlarge the crate's supported API surface to buy test reach. The
proptests reach them already, and they run in **every** CI test leg rather than
weekly — so for these two the in-crate form is the stronger check, not the
weaker one. Both were verified against pre-fix code the same way a fuzz target is.

## Running Fuzz Targets

### Quick run (30 seconds)

```bash
# fraiseql-core targets
cd crates/fraiseql-core
cargo +nightly fuzz run graphql_parser \
  fuzz/corpus/graphql_parser fuzz/seed_corpus/graphql_parser \
  -- -max_total_time=30

# fraiseql-wire targets
cd crates/fraiseql-wire
cargo +nightly fuzz run protocol_decode \
  fuzz/corpus/protocol_decode fuzz/seed_corpus/protocol_decode \
  -- -max_total_time=30
```

The first corpus directory (`fuzz/corpus/`) is the writable working corpus (gitignored). The second (`fuzz/seed_corpus/`) contains hand-crafted seeds (committed to git).

### Extended run (1 hour)

```bash
cd crates/fraiseql-core
cargo +nightly fuzz run graphql_parser \
  fuzz/corpus/graphql_parser fuzz/seed_corpus/graphql_parser \
  -- -max_total_time=3600 -max_len=65536 -rss_limit_mb=2048
```

### Run every target in a crate

`cargo fuzz list` enumerates a crate's targets, so the loop does not have to be
kept in sync with the crate by hand:

```bash
cd crates/fraiseql-core
for target in $(cargo +nightly fuzz list); do
  cargo +nightly fuzz run "$target" \
    "fuzz/corpus/$target" "fuzz/seed_corpus/$target" -- -max_total_time=600
done
```

### Build-verify every target

Targets drift out of date with the API they exercise, and a target that does not
build is silently absent from the campaign. To check all of them at once —
`fuzz build` with no target name builds every target in the crate:

```bash
for c in fraiseql-arrow fraiseql-auth fraiseql-core fraiseql-db \
         fraiseql-federation fraiseql-secrets fraiseql-server fraiseql-wire; do
  (cd "crates/$c" && cargo +nightly fuzz build) || echo "FAILED: $c"
done
```

## Investigating Crashes

When a fuzzer finds a crash, the failing input is saved to `fuzz/artifacts/<target>/`:

```bash
# Reproduce a crash
cargo +nightly fuzz run graphql_parser fuzz/artifacts/graphql_parser/crash-abc123

# Get a minimized test case
cargo +nightly fuzz tmin graphql_parser fuzz/artifacts/graphql_parser/crash-abc123
```

## Seed Corpus

Each target has a `seed_corpus/<target>/` directory with hand-crafted inputs that cover common patterns:

- **Valid inputs** — exercise happy paths (valid queries, schemas, TOML)
- **Invalid inputs** — exercise error handling (malformed syntax, injection attempts)
- **Edge cases** — boundary conditions (empty input, deep nesting, huge values)

The fuzzer uses these as starting points and mutates them to discover new code paths.

## Adding a New Fuzz Target

1. Create `fuzz/fuzz_targets/<name>.rs`:

   ```rust
   #![no_main]
   use libfuzzer_sys::fuzz_target;

   fuzz_target!(|data: &str| {
       // Your fuzzing logic here
   });
   ```

2. Register in `fuzz/Cargo.toml`:

   ```toml
   [[bin]]
   name = "<name>"
   path = "fuzz_targets/<name>.rs"
   doc = false
   ```

3. Create seed corpus in `fuzz/seed_corpus/<name>/`

4. Add to `.github/workflows/fuzz.yml` matrix

5. **Verify it finds a bug you already know about.** Point the target at the
   pre-fix code — revert the fix locally, or check out the parent commit — and
   watch it crash. Then restore and watch it pass.

   This step is not optional and it is not ceremony. A fuzz target that cannot
   find its own original defect asserts nothing, but reports green forever, which
   is worse than having no target at all: it occupies the name a real one would
   have taken. The `complexity` target's #976 seeds were verified this way, and
   the check paid for itself immediately — it showed that the `catch_unwind` in
   `parse_graphql_document` does *not* make the target green, because
   `libfuzzer-sys` installs a panic hook that aborts before unwinding. The guard
   that rejects the input is what makes it green. Without the revert check, the
   wrong mechanism would have been credited with the fix.

## CI

Fuzzing runs weekly on GitHub Actions (Sundays at 04:00 UTC), 900 seconds per
target with a 2 GB memory limit. A manual dispatch defaults to 1800 seconds and
takes a `max_total_time` input. Crash artifacts are uploaded on failure, and the
corpus is cached between runs so coverage compounds.

To trigger manually: Actions → Fuzz Testing → Run workflow.

### It is an assurance signal, not a merge gate

The campaign runs only on a schedule and on manual dispatch — never on push or
pull request — and the `dev` ruleset requires only `preflight` and `security`.
Nothing here can block a merge. That is deliberate: a fuzzer finds real bugs on
its own clock, and gating merges on a stochastic search trains people to ignore
it.

The opposite hazard is the one that actually bit. Between 2026-06-21 and
2026-08-02 every scheduled run was red with a genuine, unauthenticated,
client-reachable parser panic (#976) and nobody looked, because a red job in a
scheduled workflow is not a signal anyone receives. Three things changed as a
result:

- **A crash opens an issue** labelled `fuzz-crash`, or comments on the open one
  for that target. A red job is no longer the only notification.
- **Build failure and crash find are separate steps.** On 2026-07-26 that day's
  nightly hit an internal compiler error and two targets failed to *build* —
  which, in the job list, looked exactly like a finding. The nightly toolchain is
  pinned (`FUZZ_NIGHTLY` in the workflow) for the same reason: a campaign that
  changes compiler underneath itself every week cannot tell a new bug from a new
  toolchain. Bump it deliberately.
- **Reproducers are committed as seeds.** A fixed crash is re-tested from
  `fuzz/seed_corpus/` on every run, rather than depending on a 90-day artifact
  cache surviving.

### Triaging a finding

Check whether the target reached FraiseQL code or died inside a dependency —
and do not stop at the latter. #976 was a panic inside `graphql-parser`, an
unmaintained third-party crate, and was still a real denial-of-service reachable
from an unauthenticated request. The fix was to stop handing the parser input it
cannot survive.

## Key Flags

| Flag | Purpose |
|------|---------|
| `-max_total_time=N` | Stop after N seconds |
| `-max_len=N` | Maximum input size in bytes |
| `-rss_limit_mb=N` | Kill if RSS exceeds N MB |
| `-jobs=N` | Run N fuzzing jobs in parallel |
| `-workers=N` | Number of worker processes |
