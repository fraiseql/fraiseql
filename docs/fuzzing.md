# Fuzzing Guide

FraiseQL uses [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz) (libFuzzer) to find panics, correctness bugs, and potential security issues in parsing and code generation.

## Prerequisites

```bash
# Install nightly toolchain (required by cargo-fuzz)
rustup toolchain install nightly

# Install cargo-fuzz
cargo install cargo-fuzz
```

## Fuzz Targets

### fraiseql-core (6 targets)

| Target | What It Fuzzes | Correctness Checks |
|--------|---------------|-------------------|
| `graphql_parser` | GraphQL query parsing | JSON roundtrip, error quality |
| `schema_deser` | Schema JSON deserialization | Roundtrip + structural equality |
| `sql_codegen` | WHERE clause → SQL generation | Balanced parens/quotes |
| `schema_compile` | Schema compilation pipeline | Roundtrip, name validation |
| `toml_config` | TOML configuration parsing | Serialization check |
| `query_variables` | Query variable definitions | Name/type invariants |

### fraiseql-wire (2 targets)

| Target | What It Fuzzes | Correctness Checks |
|--------|---------------|-------------------|
| `protocol_decode` | PostgreSQL wire protocol | Consumed bytes bounds |
| `scram_parse` | SCRAM-SHA-256 messages | RFC 5802 format, error quality |

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

### Run all targets

```bash
# fraiseql-core
for target in graphql_parser schema_deser sql_codegen schema_compile toml_config query_variables; do
  cd crates/fraiseql-core
  cargo +nightly fuzz run "$target" "fuzz/seed_corpus/$target" -- -max_total_time=600
  cd ../..
done

# fraiseql-wire
for target in protocol_decode scram_parse; do
  cd crates/fraiseql-wire
  cargo +nightly fuzz run "$target" "fuzz/seed_corpus/$target" -- -max_total_time=600
  cd ../..
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

## CI

Fuzzing runs weekly on GitHub Actions (Sundays at 03:00 UTC). Each target gets 1 hour of fuzzing time with a 2 GB memory limit. Crash artifacts are uploaded automatically on failure.

To trigger manually: Actions → Fuzz Testing → Run workflow.

## Key Flags

| Flag | Purpose |
|------|---------|
| `-max_total_time=N` | Stop after N seconds |
| `-max_len=N` | Maximum input size in bytes |
| `-rss_limit_mb=N` | Kill if RSS exceeds N MB |
| `-jobs=N` | Run N fuzzing jobs in parallel |
| `-workers=N` | Number of worker processes |
