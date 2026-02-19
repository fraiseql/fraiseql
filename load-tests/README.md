# FraiseQL Load Testing with K6

Comprehensive load testing infrastructure for FraiseQL using [k6](https://k6.io/), a modern load testing tool designed for performance engineering.

## Table of Contents

- [Installation](#installation)
- [Quick Start](#quick-start)
- [Test Scenarios](#test-scenarios)
- [Configuration](#configuration)
- [Running Tests](#running-tests)
- [Interpreting Results](#interpreting-results)
- [Performance Baselines](#performance-baselines)
- [Best Practices](#best-practices)

## Installation

### Prerequisites

- K6 v0.49.0 or later
- FraiseQL server running and accessible
- Node.js (optional, for helper scripts)

### Install K6

**macOS (Homebrew):**
```bash
brew install k6
```

**Linux (Debian/Ubuntu):**
```bash
sudo apt-key adv --keyserver hkp://keyserver.ubuntu.com:80 --recv-keys C5AD17C747E3415A3642D57D77C6C491D6AC1D69
echo "deb https://dl.k6.io/deb stable main" | sudo tee /etc/apt/sources.list.d/k6-stable.list
sudo apt-get update
sudo apt-get install k6
```

**Linux (Arch):**
```bash
sudo pacman -S k6
```

**Docker:**
```bash
docker pull grafana/k6:latest
```

**Verify Installation:**
```bash
k6 version
```

## Quick Start

### 1. Start FraiseQL Server

Ensure your FraiseQL server is running:
```bash
# From fraiseql root directory
cargo run --bin fraiseql-server
```

Server should be accessible at `http://localhost:8000`

### 2. Run Your First Load Test

```bash
# Run mixed workload test (good starting point)
k6 run load-tests/k6/scenarios/mixed-workload.js

# Or with custom endpoint
ENDPOINT=http://localhost:8000/graphql k6 run load-tests/k6/scenarios/mixed-workload.js
```

### 3. View Results

K6 prints results to console. Key metrics to watch:

```
✓ status is 200
✓ response time < 500ms

     data_received..........: 15 MB
     data_sent.............: 5.2 MB
     http_req_blocked.......: avg=10ms    min=5ms     med=8ms     max=50ms    p(90)=20ms   p(95)=30ms   p(99)=45ms
     http_req_connecting....: avg=2ms     min=0s      med=0s      max=20ms    p(90)=5ms    p(95)=10ms   p(99)=18ms
     http_req_duration......: avg=45ms    min=10ms    med=35ms    max=500ms   p(90)=80ms   p(95)=120ms  p(99)=250ms
     http_req_failed........: 0.5%
     http_req_receiving.....: avg=5ms     min=1ms     med=3ms     max=15ms    p(90)=8ms    p(95)=10ms   p(99)=12ms
     http_req_sending.......: avg=2ms     min=1ms     med=2ms     max=10ms    p(90)=3ms    p(95)=4ms    p(99)=8ms
     http_req_tls_handshaking: avg=0s      min=0s      med=0s      max=0s      p(90)=0s     p(95)=0s     p(99)=0s
     http_req_waiting.......: avg=35ms    min=5ms     med=28ms    max=480ms   p(90)=70ms   p(95)=110ms  p(99)=240ms
     http_reqs.............: 50000 3333.33/s
     iteration_duration.....: avg=50ms    min=11ms    med=40ms    max=520ms   p(90)=85ms   p(95)=125ms  p(99)=255ms
     iterations............: 50000 3333.33/s
     vus...................: 50      min=50      max=50
     vus_max...............: 100     min=100     max=100
```

## Test Scenarios

### 1. Mixed Workload (`mixed-workload.js`)

**Purpose:** Realistic production traffic pattern

**Characteristics:**
- 80% read operations (queries)
- 15% write operations (mutations)
- 5% health checks
- Two traffic patterns: daytime load and burst spikes

**Scenarios:**
- `daytime_load`: Gradual ramp to 1000 rps, 5 min sustained
- `burst_traffic`: Sudden spike to 2000 rps for 1 minute

**Run:**
```bash
k6 run load-tests/k6/scenarios/mixed-workload.js
```

**Metrics to Watch:**
- p95 response time (should be <50ms)
- Error rate (should be <1%)
- p99 response time (should be <200ms)

### 2. GraphQL Queries (`graphql-queries.js`)

**Purpose:** Read-heavy workload performance

**Characteristics:**
- Tests 4 query types: simple, nested, complex, list
- Emphasizes query parsing and execution performance
- High throughput baseline test

**Scenarios:**
- `sustained_load`: 1000 rps for 5 minutes
- `spike_test`: Rapid escalation 100→5000→100 rps

**Run:**
```bash
k6 run load-tests/k6/scenarios/graphql-queries.js
```

**Use When:**
- Benchmarking query execution
- Testing query caching effectiveness
- Validating GraphQL parsing performance

### 3. GraphQL Mutations (`graphql-mutations.js`)

**Purpose:** Write operation stress testing

**Characteristics:**
- Tests create, update, delete operations
- Focuses on database write performance
- Lower throughput than queries (expected)

**Scenarios:**
- `sustained_mutations`: 200 rps sustained
- `create_heavy`: Focused create operation test at 150 rps

**Run:**
```bash
k6 run load-tests/k6/scenarios/graphql-mutations.js
```

**Use When:**
- Benchmarking mutation throughput
- Testing database write handling
- Validating transaction performance

### 4. Authentication Flow (`auth-flow.js`)

**Purpose:** Security and session management testing

**Characteristics:**
- 60% session validation (cheapest operation)
- 25% login attempts
- 15% token refresh
- Tests auth endpoint latency and rate limiting

**Scenarios:**
- `baseline_auth`: Normal auth traffic at 100 rps
- `brute_force_stress`: Simulated brute force attacks at 300 rps
- `token_refresh_burst`: Sudden refresh spike (tokens nearing expiry)

**Run:**
```bash
k6 run load-tests/k6/scenarios/auth-flow.js
```

**Use When:**
- Testing rate limiting behavior
- Validating token management
- Stress testing session handling

### 5. APQ Cache Effectiveness (`apq-cache.js`)

**Purpose:** Automatic Persisted Query (APQ) performance validation

**Characteristics:**
- Tests cache hit vs. miss performance
- Demonstrates bandwidth savings with APQ
- Tracks cache warming behavior

**Scenarios:**
- `cache_warmup`: New query registration phase
- `warm_cache`: Mostly cache hits, high load
- `cache_with_new_queries`: Mix of hits and misses

**Run:**
```bash
k6 run load-tests/k6/scenarios/apq-cache.js
```

**Expected Results:**
- Cache hits: <50ms
- Cache misses: >100ms (query registration)
- 5-10x performance improvement for cached queries

**Use When:**
- Validating APQ implementation
- Measuring bandwidth savings
- Testing cache effectiveness

## Configuration

### Environment Variables

All scenarios support configuration via environment variables:

```bash
# Custom endpoint
ENDPOINT=http://localhost:8000/graphql k6 run scenarios/mixed-workload.js

# With authentication token
AUTH_TOKEN=eyJhbGc... k6 run scenarios/mixed-workload.js

# Both
ENDPOINT=https://production.example.com/graphql \
AUTH_TOKEN=secret_token \
k6 run scenarios/mixed-workload.js
```

### Shared Configuration (`k6/config.js`)

Contains helper functions:

```javascript
// Get configured endpoint
const endpoint = getEndpoint();

// Execute GraphQL query
const response = executeGraphQL(query, variables);

// Validate response structure
validateGraphQLResponse(response, check);

// APQ-based execution
const response = executeGraphQLWithAPQ(query, variables);

// Utility helpers
randomEmail()          // Generate test email
randomString(10)       // Generate random string
randomInt(1, 100)      // Random number in range
randomChoice([...])    // Pick random item from array
```

### Performance Thresholds (`k6/thresholds.js`)

Predefined threshold sets for different test types:

```javascript
import { getThresholdsForTest } from './thresholds.js';

export const options = {
  thresholds: getThresholdsForTest('query'),
};
```

Available threshold sets:
- `defaultThresholds` - p50<10ms, p95<50ms, p99<200ms
- `mutationThresholds` - Relaxed for writes
- `tightThresholds` - p50<5ms (auth operations)
- `spikeThresholds` - Relaxed for stress tests
- `apqThresholds` - Cache hit/miss tracking

## Running Tests

### Basic Execution

```bash
# Run with default settings
k6 run scenarios/mixed-workload.js

# Run with verbose output
k6 run --verbose scenarios/mixed-workload.js

# Run specific scenario
k6 run --stage duration:30s scenarios/mixed-workload.js
```

### Advanced Options

```bash
# Set virtual users (overrides scenario definition)
k6 run --vus 100 --duration 1m scenarios/mixed-workload.js

# Output results to file
k6 run scenarios/mixed-workload.js --out json=results.json

# Run with custom log level
k6 run scenarios/mixed-workload.js --log-output=stdout
```

### Docker Execution

```bash
# Run k6 in Docker
docker run -v $(pwd):/load-tests grafana/k6 run /load-tests/k6/scenarios/mixed-workload.js

# With environment variables
docker run \
  -v $(pwd):/load-tests \
  -e ENDPOINT=http://host.docker.internal:8000/graphql \
  grafana/k6 run /load-tests/k6/scenarios/mixed-workload.js
```

### Continuous Integration

Example GitHub Actions workflow:

```yaml
name: Load Testing

on: [push, pull_request]

jobs:
  load-test:
    runs-on: ubuntu-latest

    services:
      fraiseql:
        image: fraiseql:latest
        ports:
          - 8000:8000

    steps:
      - uses: actions/checkout@v2

      - name: Install k6
        run: |
          sudo apt-key adv --keyserver hkp://keyserver.ubuntu.com --recv-keys C5AD17C747E3415A3642D57D77C6C491D6AC1D69
          echo "deb https://dl.k6.io/deb stable main" | sudo tee /etc/apt/sources.list.d/k6-stable.list
          sudo apt-get update && sudo apt-get install k6

      - name: Run load tests
        run: k6 run load-tests/k6/scenarios/mixed-workload.js
        env:
          ENDPOINT: http://localhost:8000/graphql
```

## Interpreting Results

### Key Metrics Explained

**http_req_duration**
- Time from request send to response received
- Critical for user experience
- Should have tight p95/p99 constraints

**http_req_failed**
- Percentage of failed requests (5xx, timeouts, etc.)
- Target: <1% for production workloads
- Indicates system stability

**http_reqs**
- Total requests completed
- Indicates throughput capacity
- Use for capacity planning

**Percentiles (p50, p95, p99)**
- p50: Median response time (what most users experience)
- p95: 95th percentile (experience of worst 5% of users)
- p99: 99th percentile (rare slow requests)

Example interpretation:
```
http_req_duration......: avg=45ms    min=10ms    med=35ms    max=500ms   p(95)=120ms   p(99)=250ms
```

✓ Good: p95=120ms, p99=250ms (within thresholds)
✓ Fast median: 35ms
✗ Possible issue: max=500ms (investigate outliers)

### Response Time Benchmarks

For FraiseQL, typical values should be:

| Query Type | p50 | p95 | p99 |
|-----------|-----|-----|-----|
| Simple Query | 5ms | 15ms | 50ms |
| Nested Query | 10ms | 30ms | 100ms |
| Complex Query | 20ms | 75ms | 200ms |
| Mutation | 30ms | 100ms | 300ms |
| Auth (login) | 50ms | 150ms | 500ms |
| APQ Cache Hit | <5ms | <20ms | <50ms |
| APQ Cache Miss | 50ms | 100ms | 300ms |

### Detecting Performance Issues

**High Error Rate (>1%)**
- Check server logs for errors
- Verify database connectivity
- Assess resource limits (CPU, memory)

**Increasing Latency Over Time**
- Possible connection pool exhaustion
- Memory leak in server
- Database connection timeout

**Spike Test Failures**
- Server can't handle burst traffic
- Consider load balancing/scaling
- Review connection pooling settings

**APQ Cache Misses Too High**
- Cache warmup incomplete
- Client not persisting queries
- Cache eviction happening

## Performance Baselines

### Creating a Baseline

Before optimizations, establish a baseline:

```bash
# Run mixed workload multiple times
for i in {1..3}; do
  k6 run load-tests/k6/scenarios/mixed-workload.js --out json=baseline-run-$i.json
done

# Combine results
cat baseline-run-*.json > baseline-combined.json
```

### Tracking Baselines Over Time

Create a baseline tracking script:

```bash
#!/bin/bash
# baseline.sh - Track performance baselines

TIMESTAMP=$(date +%Y-%m-%d-%H%M%S)
BASELINE_DIR="load-tests/baselines/$TIMESTAMP"
mkdir -p "$BASELINE_DIR"

for scenario in mixed-workload graphql-queries graphql-mutations auth-flow apq-cache; do
  echo "Running: $scenario"
  k6 run "load-tests/k6/scenarios/$scenario.js" \
    --out json="$BASELINE_DIR/$scenario.json"
done

echo "Baseline results saved to: $BASELINE_DIR"
```

### Comparing Against Baseline

```bash
# Run current test
k6 run scenarios/mixed-workload.js --out json=current.json

# Compare with baseline (manual inspection of JSON)
jq '.metrics.http_req_duration' baseline.json current.json
```

### Commit Baselines to Git

After establishing baselines, commit them:

```bash
# Create baseline snapshot
mkdir -p load-tests/baselines
cp results.json load-tests/baselines/baseline-$(date +%Y%m%d).json

# Commit
git add load-tests/baselines/
git commit -m "perf: establish load testing baseline

Results:
- Mixed workload: 3333 rps, p95=120ms
- Query stress: 5000 rps, p95=50ms
- Mutation: 200 rps, p95=100ms
- APQ cache: 5-10x improvement on cache hits

Baselines committed for regression detection."
```

## Best Practices

### 1. Progressive Load Increase

Start small, increase gradually:
```bash
# Test with low load first
k6 run --vus 10 --duration 1m scenarios/mixed-workload.js

# Then ramp up
k6 run --vus 100 --duration 5m scenarios/mixed-workload.js

# Only then do full load test
k6 run scenarios/mixed-workload.js
```

### 2. Isolate Variables

Test one thing at a time:

```bash
# Test queries only
k6 run scenarios/graphql-queries.js

# Test mutations only
k6 run scenarios/graphql-mutations.js

# Then test mixed
k6 run scenarios/mixed-workload.js
```

### 3. Long-Running Tests

For endurance testing:

```bash
k6 run \
  --stage 5m:0 \        # Ramp up over 5 min
  --stage 30m:100vus \  # Hold at 100 vus for 30 min
  --stage 5m:0 \        # Ramp down over 5 min
  scenarios/mixed-workload.js
```

### 4. Monitor Server During Tests

In separate terminal:

```bash
# Watch CPU/Memory
watch -n 1 'ps aux | grep fraiseql-server'

# Watch database connections
# (depends on database type)

# Watch logs
tail -f server.log
```

### 5. Post-Test Analysis

After running tests:

1. Export results to JSON
2. Load into metrics tools (Grafana, DataDog, etc.)
3. Create performance reports
4. Document findings

```bash
# Export to JSON for analysis
k6 run scenarios/mixed-workload.js --out json=results.json

# Use k6 official analyzers or import to your metrics platform
```

### 6. Test Environment

Keep test environment consistent:

```bash
# Use Docker for reproducibility
docker-compose -f docker/docker-compose.yml up

# Run tests against Docker environment
ENDPOINT=http://localhost:8000/graphql k6 run scenarios/mixed-workload.js
```

### 7. Error Analysis

When tests fail thresholds:

```bash
# Run with full logging
k6 run \
  --log-output=stdout \
  --verbose \
  scenarios/mixed-workload.js

# Look for patterns in error responses
k6 run \
  --out json=results.json \
  scenarios/mixed-workload.js

# Analyze error rate by scenario
jq '.metrics | keys | .[]' results.json | grep failed
```

## Troubleshooting

### "Connection refused" errors

```bash
# Verify server is running
curl http://localhost:8000/graphql

# Check ENDPOINT variable
echo $ENDPOINT

# Try explicit endpoint
ENDPOINT=http://localhost:8000/graphql k6 run scenarios/mixed-workload.js
```

### High memory usage during test

```bash
# Reduce concurrent VUs
k6 run --vus 50 scenarios/mixed-workload.js

# Run shorter duration
k6 run --duration 1m scenarios/mixed-workload.js
```

### "Too many open files" error

Increase file descriptor limit:
```bash
ulimit -n 65536
```

### Inconsistent results between runs

- Warm up the server first
- Use identical test data
- Run on idle machine
- Eliminate network jitter
- Use consistent testing methodology

## Advanced Topics

### Custom Scenarios

Create your own scenario file:

```javascript
import { getEndpoint, executeGraphQL, validateGraphQLResponse } from "../config.js";
import { defaultThresholds } from "../thresholds.js";

export const options = {
  thresholds: defaultThresholds,
  scenarios: {
    custom: {
      executor: "ramping-arrival-rate",
      stages: [
        { duration: "5m", target: 1000 },
      ],
    },
  },
};

export default function() {
  const query = `query { customOperation { result } }`;
  const response = executeGraphQL(query);
  validateGraphQLResponse(response, check);
}
```

### Integration with CI/CD

See [Example CI/CD](#continuous-integration) section above.

### Performance Profiling

Export full metrics:
```bash
k6 run \
  --out json=metrics.json \
  --out csv=metrics.csv \
  scenarios/mixed-workload.js
```

Analyze with tools like:
- [k6 Results](https://k6.io/docs/results-output/)
- Grafana
- Datadog
- Custom Python/Node scripts

---

**Questions?** Check [k6 documentation](https://k6.io/docs/) or open an issue in the FraiseQL repository.
