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

## The instruction-count gate (required)

The `perf (instructions)` check is the one benchmark that can stop a merge (#1636). It
runs `crates/fraiseql-core/benches/executor_instructions.rs` (`Executor::execute`: root
list, by-id, nested; lean and fat rows) and
`crates/fraiseql-server/benches/rest_instructions.rs` (REST list and by-id `GET`) under
callgrind, nine times. It then compares each bench's mean instruction count with
`benchmarks/instructions-baseline.json`.

The check fails if a bench moves more than 3% in either direction.

- **Up 3% or more:** the change made the engine do more work per request.
- **Down 3% or more:** the baseline has to come down with it. Otherwise it would hide
  the next regression, up to the size of the improvement.

Counts are deterministic only per binary and container. Re-bless the baseline only with
the leg itself, never with a local run:

```bash
# Run the leg locally (needs the Dagger engine); compare against the committed baseline
dagger call perf-instructions --source=.

# Print the measured counts as a new baseline file
dagger call perf-instructions --source=. --print > benchmarks/instructions-baseline.json
```

When a change is *meant* to move a bench past 3%, for example an engine optimisation:

1. Commit the new baseline in the same commit as the change. A change that moves the
   counts and leaves the baseline does not pass.
2. In the commit body, list each bench's old and new counts and say why they moved.

The gate measures nothing without its symbols, so `[profile.bench]` keeps
`strip = false`. The comparator (`tools/check-perf-instructions.py`) refuses a count of
0 rather than write it into a baseline.

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
# Cargo.toml
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
