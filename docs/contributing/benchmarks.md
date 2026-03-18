# Benchmarks

FraiseQL uses [Criterion.rs](https://bheisler.github.io/criterion.rs/book/) for micro-benchmarks and [k6](https://k6.io/) for load tests. CI gates regressions automatically.

## Quick Reference

```bash
# Run all benchmarks
cargo bench --workspace

# Run a specific crate's benchmarks
cargo bench -p fraiseql-core

# Run a single benchmark by name
cargo bench -p fraiseql-core -- "projection"

# Save a named baseline for comparison
cargo bench --workspace -- --save-baseline my-feature

# Compare two baselines
critcmp before after
```

## CI Regression Detection

The `.github/workflows/bench.yml` workflow runs on every push to `dev`/`main` and on PRs:

1. **On `dev` push**: Runs all benchmarks and saves the result as the `dev` baseline in GitHub Actions cache.
2. **On PR**: Restores the most recent `dev` baseline, runs benchmarks as the `pr` baseline, and compares.

### Thresholds

| Category | Threshold | Examples |
|----------|-----------|---------|
| **Micro** (pure computation) | 5% | SQL projection, federation, saga, cache |
| **Slow** (DB-connected) | 15% | Row processing, HTTP pipeline, pagination |

Regressions beyond these thresholds produce a `::warning` annotation in the PR. They are **advisory, not blocking** — CI runner hardware variance can cause false positives.

### Benchmark Categories

The `critcmp` filter patterns in CI:

- **Micro**: `projection`, `federation`, `design_analysis`, `saga`, `typename`, `payload_size`, `complete_pipeline`, `cache_concurrent`, `cache_get_latency`
- **Slow**: `10k_rows`, `100k_rows`, `1m_rows`, `where_clause`, `pagination`, `http_response_pipeline`, `graphql_transform`, `god_objects`

## Adding New Benchmarks

1. Add the benchmark to the appropriate crate's `benches/` directory.
2. Register it in the crate's `Cargo.toml`:

```toml
[[bench]]
name = "my_benchmark"
harness = false
```

3. If the benchmark falls into the micro or slow category, add its name pattern to the appropriate `critcmp -f` filter in `.github/workflows/bench.yml`.

4. Run locally to verify:

```bash
cargo bench -p fraiseql-core -- --save-baseline before
# Make your changes
cargo bench -p fraiseql-core -- --save-baseline after
critcmp before after
```

## Benchmark Suites

| Crate | File | What It Measures |
|-------|------|------------------|
| `fraiseql-core` | `adapter_comparison.rs` | DB adapter overhead per database type |
| `fraiseql-core` | `design_analysis.rs` | Schema compilation and analysis |
| `fraiseql-core` | `full_pipeline_comparison.rs` | End-to-end query pipeline |
| `fraiseql-server` | `performance_benchmarks.rs` | HTTP layer and GraphQL handling |
| `fraiseql-wire` | `micro_benchmarks.rs` | Wire protocol encoding/decoding |
| `fraiseql-arrow` | `arrow_vs_json_serialization.rs` | Arrow vs JSON serialization |

## Load Tests (k6)

The `.github/workflows/perf-baseline.yml` workflow runs k6 load tests:

- Triggered on pushes to `main`/`dev` and PRs labeled `perf`
- Builds a release binary, starts the server, and runs `benchmarks/load/basic.js`
- Results are archived as artifacts for 90 days

```bash
# Run locally (requires a running FraiseQL server)
k6 run benchmarks/load/basic.js -e BASE_URL=http://localhost:8815
```

## Installing Tools

```bash
# Install critcmp for comparing baselines
cargo install critcmp --locked

# Install k6 (Arch Linux)
sudo pacman -S k6

# Install k6 (Ubuntu/Debian)
sudo apt-get install k6
```
