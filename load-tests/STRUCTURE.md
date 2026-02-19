# K6 Load Testing Infrastructure Structure

## Directory Layout

```
load-tests/
├── README.md                           # Comprehensive guide (installation, running tests, interpreting results)
├── STRUCTURE.md                        # This file
└── k6/
    ├── config.js                       # Shared configuration and helpers
    ├── thresholds.js                   # Performance thresholds and pass/fail criteria
    └── scenarios/
        ├── graphql-queries.js          # Read-heavy workload testing (1000+ rps)
        ├── graphql-mutations.js        # Write operation stress test (200 rps)
        ├── mixed-workload.js           # Realistic production pattern (80% reads, 15% writes, 5% health)
        ├── auth-flow.js                # Authentication endpoint stress test
        └── apq-cache.js                # APQ cache effectiveness validation
```

## File Purposes

### config.js
**Shared configuration and utilities**
- Environment-based endpoint configuration
- GraphQL request execution helpers
- APQ (Automatic Persisted Query) support
- Response validation utilities
- Test data generators (random email, string, ID selection)
- Helper functions for common operations

### thresholds.js
**Performance baseline definitions**
- `defaultThresholds`: p50<10ms, p95<50ms, p99<200ms (queries)
- `mutationThresholds`: Relaxed for write operations
- `tightThresholds`: p50<5ms (authentication)
- `spikeThresholds`: Relaxed for stress testing
- `apqThresholds`: Cache hit/miss tracking

**Functions:**
- `combineThresholds()`: Merge multiple threshold sets
- `getThresholdsForTest()`: Select thresholds by test type
- `createScenarioThresholds()`: Custom scenario thresholds

### graphql-queries.js
**Read-heavy workload testing**
- **Purpose**: Validate query performance and throughput
- **Query Types**: simple (5ms), nested (10ms), complex (20ms), list
- **Scenarios**:
  - `sustained_load`: 1000 rps for 5 minutes
  - `spike_test`: 100→5000→100 rps escalation
- **Metrics Focus**: Query parse/execute time, caching

### graphql-mutations.js
**Write operation stress testing**
- **Purpose**: Validate mutation throughput and database write performance
- **Operations**: Create (40%), Update (40%), Delete (20%)
- **Scenarios**:
  - `sustained_mutations`: 200 rps sustained
  - `create_heavy`: Focused create operations at 150 rps
- **Metrics Focus**: Write latency, database transaction time

### mixed-workload.js
**Realistic production traffic pattern**
- **Purpose**: Validate system under realistic conditions
- **Traffic Distribution**:
  - 80% Queries (reads)
  - 15% Mutations (writes)
  - 5% Health checks
- **Scenarios**:
  - `daytime_load`: Normal traffic pattern (1000 rps peak)
  - `burst_traffic`: Sudden spikes (2000 rps)
- **Metrics Focus**: Overall system behavior, scaling

### auth-flow.js
**Authentication endpoint stress testing**
- **Purpose**: Validate auth security and rate limiting
- **Operations**:
  - 60% Session validation (cheapest)
  - 25% Login attempts
  - 15% Token refresh
- **Scenarios**:
  - `baseline_auth`: Normal auth load (100 rps)
  - `brute_force_stress`: Simulated attacks (300 rps)
  - `token_refresh_burst`: Token expiry spikes
- **Metrics Focus**: Login latency, rate limit effectiveness

### apq-cache.js
**Automatic Persisted Query cache effectiveness**
- **Purpose**: Validate APQ implementation and bandwidth savings
- **Query Distribution**: Weighted by frequency (40%, 35%, 20%, 5%)
- **Scenarios**:
  - `cache_warmup`: New query registration
  - `warm_cache`: Mostly cache hits (500 rps)
  - `cache_with_new_queries`: Mixed hits/misses
- **Metrics Focus**: Cache hit rate, bandwidth savings, speed improvement
- **Expected**: 5-10x faster on cache hits

## Usage Quick Reference

### Run a single scenario
```bash
k6 run load-tests/k6/scenarios/mixed-workload.js
```

### Run with custom endpoint
```bash
ENDPOINT=http://localhost:8000/graphql k6 run load-tests/k6/scenarios/mixed-workload.js
```

### Run with authentication
```bash
ENDPOINT=https://api.example.com/graphql \
AUTH_TOKEN=eyJhbGc... \
k6 run load-tests/k6/scenarios/mixed-workload.js
```

### Output results as JSON for analysis
```bash
k6 run \
  --out json=results.json \
  load-tests/k6/scenarios/mixed-workload.js
```

### Run on Docker
```bash
docker run -v $(pwd):/load-tests grafana/k6 run /load-tests/k6/scenarios/mixed-workload.js
```

## Performance Baselines

### Recommended Test Order

1. **Authentication** - Establish auth baseline
   ```bash
   k6 run load-tests/k6/scenarios/auth-flow.js
   ```

2. **Queries** - Establish read baseline
   ```bash
   k6 run load-tests/k6/scenarios/graphql-queries.js
   ```

3. **Mutations** - Establish write baseline
   ```bash
   k6 run load-tests/k6/scenarios/graphql-mutations.js
   ```

4. **Mixed Workload** - Realistic scenario
   ```bash
   k6 run load-tests/k6/scenarios/mixed-workload.js
   ```

5. **APQ Cache** - Optimization validation
   ```bash
   k6 run load-tests/k6/scenarios/apq-cache.js
   ```

## Typical Performance Benchmarks

| Test | Metric | Target | Status |
|------|--------|--------|--------|
| Simple Query | p95 | <15ms | ✓ |
| Nested Query | p95 | <50ms | ✓ |
| Complex Query | p95 | <75ms | ✓ |
| Mutation | p95 | <100ms | ✓ |
| Auth Login | p95 | <150ms | ✓ |
| APQ Cache Hit | p95 | <20ms | ✓ |
| Mixed Workload | p95 | <50ms | ✓ |
| Error Rate | <1% | <1% | ✓ |

## Key Features

### Configuration Management
- Environment-based endpoint and auth token
- Per-scenario threshold customization
- Threshold presets for different test types

### Test Utilities
- GraphQL request execution
- APQ protocol support
- Response validation
- Random test data generation
- Timing extraction and analysis

### Scenario Coverage
- Read-heavy workloads
- Write-heavy workloads
- Mixed realistic traffic
- Authentication stress
- Cache effectiveness

### Metrics Tracking
- Latency percentiles (p50, p95, p99)
- Error rates and types
- Throughput (rps)
- Cache hit/miss ratios
- Timings breakdown (connecting, waiting, receiving)

## Integration Points

### Shared Configuration
All scenarios import from `config.js`:
```javascript
import {
  getEndpoint,
  executeGraphQL,
  validateGraphQLResponse,
  randomEmail,
} from "../config.js";
```

### Threshold Selection
All scenarios use `thresholds.js`:
```javascript
import { defaultThresholds } from "../thresholds.js";
export const options = { thresholds: defaultThresholds };
```

### Result Tracking
Test setup/teardown functions log results:
```javascript
export function setup() { /* logging */ }
export function teardown(data) { /* results */ }
```

## Next Steps

1. Read [README.md](README.md) for detailed usage guide
2. Install k6: `brew install k6` (or platform equivalent)
3. Run your first test: `k6 run load-tests/k6/scenarios/mixed-workload.js`
4. Establish baselines by running all scenarios
5. Monitor and optimize based on results

---

For comprehensive documentation, see [README.md](README.md)
