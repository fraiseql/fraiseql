# Benchmarks

FraiseQL uses [Criterion.rs](https://bheisler.github.io/criterion.rs/book/) for micro-benchmarks and [k6](https://k6.io/) for load tests.

> **No benchmark gates a merge today.** The 2026-05-31 Dagger migration removed the push
> and PR triggers from every benchmark workflow, and they have not been restored: this
> project's own release notes state that its performance numbers are not yet trustworthy,
> so gating merges on them would make CI assert what the notes deny. All three workflows
> run on manual dispatch, and each takes the comparison and baseline decisions as inputs.

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

`.github/workflows/bench.yml` is **dispatch-only**. Run it from the Actions tab, or:

```bash
# Compare this commit against the stored `dev` baseline
gh workflow run bench.yml --ref <branch> -f compare_against=dev

# Establish a new baseline
gh workflow run bench.yml --ref dev -f save_baseline=true -f baseline_name=dev
```

1. Every run benchmarks the checked-out commit and saves it under the criterion baseline
   name `run`.
2. `compare_against` (default `dev`) restores that baseline from the GitHub Actions cache
   and `critcmp`s against it. The table is written to the job summary; blank skips the
   comparison. If no cached baseline of that name exists, the summary says so rather than
   reporting a clean comparison.
3. `save_baseline` stores the run under `baseline_name` for future comparisons.

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

1. If the benchmark falls into the micro or slow category, add its name pattern to the appropriate `critcmp -f` filter in `.github/workflows/bench.yml`.

2. Run locally to verify:

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

- **Dispatch-only.** `gh workflow run perf-baseline.yml --ref <branch>`
- Builds a release binary, starts the server, and runs `benchmarks/load/basic.js`
- `compare_against` (default `dev`) downloads that baseline artifact and diffs against it;
  `save_baseline` stores the run under `baseline_name`
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
