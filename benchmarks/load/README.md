# FraiseQL Load Tests

Performance baseline tests using [k6](https://k6.io/).

## Prerequisites

```bash
# Install k6 (Linux/macOS)
brew install k6        # macOS
sudo apt install k6    # Ubuntu/Debian (after adding Grafana apt repo)
```

## Scripts

| Script | Description | VUs | Thresholds |
|--------|-------------|-----|------------|
| `basic.js` | Read-path: list + single-entity + introspection | 50 | P99 < 500 ms, errors < 1% |
| `mutations.js` | Write-path: create mutations through fn_* functions | 20 | P99 < 1 000 ms, errors < 1% |

## Running Locally

**Start dependencies:**

```bash
docker compose -f docker/docker-compose.test.yml up -d postgres
```

**Compile and start the server:**

```bash
cargo build --release -p fraiseql-cli

# Compile a test schema (use your own or the example below)
./target/release/fraiseql compile benchmarks/load/test_schema.json

# Run the server
DATABASE_URL=postgres://fraiseql:fraiseql@localhost:5432/fraiseql_test \
  ./target/release/fraiseql run benchmarks/load/test_schema.compiled.json
```

**Run the load test:**

```bash
# Basic read-path test
k6 run benchmarks/load/basic.js

# With a different server URL
k6 run benchmarks/load/basic.js -e BASE_URL=http://staging.example.com

# With authentication
k6 run benchmarks/load/basic.js -e AUTH_TOKEN=eyJ...

# Save results for later analysis
k6 run benchmarks/load/basic.js --out json=results.json
```

## Interpreting Results

k6 reports the following key metrics:

- **`http_req_duration`** — End-to-end HTTP latency. Watch P95 and P99.
- **`graphql_errors`** — Fraction of requests that returned GraphQL errors (not HTTP errors).
- **`http_reqs`** — Total requests per second (throughput).

Threshold violations are printed at the end of the run:

```
✓ http_req_duration.............: avg=12ms  p(95)=45ms  p(99)=89ms
✗ graphql_errors...............: 2.3% ✗ rate<0.01
```

## Adding a Test Schema

Create `benchmarks/load/test_schema.json` with your schema definition.
The CI workflow will automatically compile and load it.

If no schema is provided, the server starts with an empty schema (introspection
only), which still validates server startup latency.
