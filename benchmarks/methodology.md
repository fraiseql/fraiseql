# Benchmark Methodology

**📍 Navigation**: [← Benchmarks](../benchmarks/) • [Performance Guide →](../docs/performance/PERFORMANCE_GUIDE.md) • [Results →](benchmark-results.md)

This document outlines the methodology used for FraiseQL performance benchmarking, ensuring reproducible and accurate measurements.

---

## Hardware & Environment

### Test System Specifications

**Primary Benchmark System**:
- **CPU**: AMD Ryzen 7 5800X (8 cores, 16 threads)
- **RAM**: 32GB DDR4-3200
- **Storage**: Samsung 980 PRO NVMe SSD (1TB)
- **OS**: Linux 6.16.6-arch1-1 (Arch Linux)
- **Kernel**: 6.16.6-arch1-1

**Cloud Benchmark System** (for production simulation):
- **Provider**: DigitalOcean
- **Instance**: Basic-2 (2 AMD vCPUs, 2GB RAM)
- **Storage**: 55GB NVMe SSD
- **Network**: 2Gbps
- **OS**: Ubuntu 22.04 LTS

### Software Versions

**Core Components**:
- **PostgreSQL**: 15.8
- **Python**: 3.13.0
- **Rust**: 1.82.0
- **FraiseQL**: v0.11.4-dev (benchmark branch)

**Python Dependencies** (key packages):
- **fastapi**: 0.115.0
- **uvicorn**: 0.32.0
- **psycopg**: 3.2.3
- **pydantic**: 2.9.0

**System Libraries**:
- **OpenSSL**: 3.4.0
- **libpq**: 15.8

---

## Database Configuration

### PostgreSQL Settings

```sql
-- Benchmark-optimized configuration
shared_buffers = '256MB'          -- 25% of system RAM
effective_cache_size = '1GB'       -- 75% of system RAM
work_mem = '16MB'                  -- Sort memory per connection
maintenance_work_mem = '128MB'     -- Maintenance operations
max_connections = 100              -- Connection pool limit
statement_timeout = '5000'         -- 5 second query timeout
idle_in_transaction_session_timeout = '30000'  -- 30 second idle timeout

-- WAL settings for performance
wal_level = minimal
fsync = off                        -- DANGEROUS: Only for benchmarks
synchronous_commit = off           -- DANGEROUS: Only for benchmarks
full_page_writes = off             -- DANGEROUS: Only for benchmarks

-- Connection settings
tcp_keepalives_idle = 60
tcp_keepalives_interval = 10
tcp_keepalives_count = 5
```

### Connection Pooling

```python
# FraiseQL connection configuration
config = FraiseQLConfig(
    database_pool_size=20,          # 20% of max_connections
    database_max_overflow=10,       # Burst capacity
    database_pool_timeout=5.0,      # Connection timeout
    database_pool_recycle=3600,     # Recycle connections hourly
)
```

---

## Test Data Setup

### Dataset Specifications

**Primary Test Dataset**:
- **Users**: 10,000 records
- **Posts**: 50,000 records (avg 5 posts per user)
- **Comments**: 100,000 records (avg 2 comments per post)
- **Categories**: 50 records
- **Tags**: 1,000 records

**Data Distribution**:
- User creation dates: Normal distribution over 2 years
- Post lengths: Pareto distribution (80% short, 20% long)
- Comment nesting: 70% top-level, 30% replies

### Schema Design

```sql
-- Core tables
CREATE TABLE tb_user (
    pk_user BIGINT PRIMARY KEY GENERATED ALWAYS AS IDENTITY,
    id UUID NOT NULL DEFAULT gen_random_uuid(),
    identifier TEXT UNIQUE,
    email TEXT UNIQUE NOT NULL,
    name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- View for GraphQL queries
CREATE VIEW v_user AS
SELECT
    pk_user,
    id::text as id,
    identifier,
    email,
    name,
    created_at,
    updated_at
FROM tb_user;

-- Table view for performance
CREATE TABLE tv_user (
    id BIGINT PRIMARY KEY,
    data JSONB GENERATED ALWAYS AS (
        jsonb_build_object(
            'id', id,
            'identifier', (SELECT identifier FROM tb_user WHERE pk_user = tv_user.id),
            'email', (SELECT email FROM tb_user WHERE pk_user = tv_user.id),
            'name', (SELECT name FROM tb_user WHERE pk_user = tv_user.id),
            'createdAt', (SELECT created_at FROM tb_user WHERE pk_user = tv_user.id),
            'posts', (
                SELECT jsonb_agg(jsonb_build_object(
                    'id', p.id,
                    'title', p.title,
                    'createdAt', p.created_at
                ) ORDER BY p.created_at DESC)
                FROM tb_post p
                WHERE p.fk_user = tv_user.id
            )
        )
    ) STORED
);
```

### Data Generation

**Synthetic Data Generation**:
```python
# Generate realistic test data
from benchmarks.data_generator import generate_test_data

# Create 10k users with realistic attributes
users = generate_test_data(
    table="tb_user",
    count=10000,
    attributes={
        "name": "realistic_name",      # Uses Faker library
        "email": "realistic_email",    # Valid email format
        "identifier": "slugify_name",  # URL-safe identifier
    }
)
```

**Data Validation**:
- All foreign keys valid
- No orphaned records
- Realistic data distributions
- Proper indexing on lookup fields

---

## Benchmark Procedures

### 1. Transformation Benchmarks

**Purpose**: Measure JSON transformation performance (Rust vs Python)

**Methodology**:
1. **Pre-generate JSON payloads** (1KB, 10KB, 100KB sizes)
2. **Warm-up phase**: 10 iterations discarded
3. **Measurement phase**: 100 iterations per test case
4. **Statistical analysis**: Mean, median, 95th percentile

**Test Cases**:
- **Simple**: 10 fields, flat structure
- **Medium**: 42 fields, moderate nesting
- **Nested**: User + 15 posts (deep relationships)
- **Large**: 100+ fields, complex nesting

**Measurement Code**:
```python
import time
from fraiseql.core.transform import transform_json

# Warm-up
for _ in range(10):
    transform_json(test_data, schema)

# Benchmark
times = []
for _ in range(100):
    start = time.perf_counter_ns()
    result = transform_json(test_data, schema)
    end = time.perf_counter_ns()
    times.append((end - start) / 1_000_000)  # Convert to milliseconds

# Statistics
import numpy as np
print(f"Mean: {np.mean(times):.4f}ms")
print(f"Median: {np.median(times):.4f}ms")
print(f"95th percentile: {np.percentile(times, 95):.4f}ms")
```

### 2. End-to-End Benchmarks

**Purpose**: Measure complete request-response cycle

**Methodology**:
1. **Start PostgreSQL** with benchmark configuration
2. **Launch FraiseQL server** with test configuration
3. **Warm-up phase**: 50 requests to populate caches
4. **Measurement phase**: 100 requests per test case
5. **Concurrent testing**: Multiple threads to simulate load

**Test Scenarios**:
- **Cold query**: No caching, first execution
- **APQ cached**: Automatic Persisted Queries enabled
- **TurboRouter**: Pre-compiled query templates
- **JSON passthrough**: Direct database JSONB output

**Load Testing**:
```bash
# Simulate concurrent users
ab -n 1000 -c 10 http://localhost:8000/graphql \
  -T 'application/json' \
  -p query_payload.json
```

### 3. Cache Performance Benchmarks

**Purpose**: Measure APQ cache effectiveness

**Methodology**:
1. **Query diversity simulation**: Generate 100 unique queries
2. **Cache warming**: Execute each query once
3. **Measurement phase**: Execute queries with 80/20 distribution
4. **Hit rate calculation**: Monitor cache statistics

**Cache Configurations Tested**:
- **Memory backend**: Fast, restart-cleared
- **PostgreSQL backend**: Persistent, multi-instance

---

## Statistical Analysis

### Measurement Accuracy

**Timing Precision**:
- **Resolution**: Nanosecond precision (`time.perf_counter_ns()`)
- **Accuracy**: ±0.001ms for sub-millisecond measurements
- **System noise**: <0.1ms variation under controlled conditions

**Statistical Methods**:
- **Sample size**: Minimum 100 measurements per test
- **Outlier removal**: 5% trimmed mean for robust statistics
- **Confidence intervals**: 95% confidence level reported
- **Distribution analysis**: Shapiro-Wilk test for normality

### Performance Variability

**Sources of Variance**:
- **System load**: CPU scheduling, I/O operations
- **Memory pressure**: Garbage collection pauses
- **Network latency**: Database connection overhead
- **Disk I/O**: Page cache warm-up

**Variance Mitigation**:
- **Dedicated benchmark system**: No other processes
- **Warm-up phases**: Stabilize performance
- **Multiple runs**: Average across 3-5 benchmark sessions
- **Controlled environment**: Consistent system state

---

## Reproducibility

### Environment Setup

**1. System Preparation**:
```bash
# Install system dependencies
sudo pacman -S postgresql python rustup  # Arch Linux
# OR
sudo apt install postgresql python3 rustc  # Ubuntu

# Configure PostgreSQL
sudo -u postgres createuser --superuser $USER
createdb fraiseql_benchmark
```

**2. Project Setup**:
```bash
# Clone and setup
git clone https://github.com/fraiseql/fraiseql.git
cd fraiseql
uv sync  # Install dependencies

# Build Rust extensions
pip install -e .[rust]
```

**3. Database Setup**:
```bash
# Start PostgreSQL with benchmark config
pg_ctl -D /tmp/pgdata start

# Create test database
createdb fraiseql_test

# Run data setup
python benchmarks/setup_test_data.py
```

### Running Benchmarks

**Transformation Benchmarks**:
```bash
# Rust vs Python transformation
uv run python benchmarks/rust_vs_python_benchmark.py

# Output: Detailed timing results with statistics
```

**End-to-End Benchmarks**:
```bash
# Requires running PostgreSQL
DATABASE_URL=postgresql://localhost/fraiseql_test \
uv run python benchmarks/database_transformation_benchmark.py

# Output: Query timing with database component breakdown
```

**Cache Benchmarks**:
```bash
# APQ cache performance
uv run python benchmarks/apq_cache_benchmark.py

# Output: Cache hit rates and timing distributions
```

### Expected Results

**System Variability Notice**:
Results may vary ±20% between different systems due to:
- CPU microarchitecture differences
- Memory speed and latency
- Storage performance
- Operating system scheduling

**Validation Criteria**:
- Rust transformation: 3-5x faster than Python
- End-to-end queries: 1.5-2.5x faster with optimizations
- Cache hit rates: 85-95% for stable query patterns

---

## Benchmark Maintenance

### Regular Validation

**Weekly Checks**:
- Run transformation benchmarks
- Verify end-to-end performance
- Check for performance regressions

**Monthly Reviews**:
- Update hardware baselines
- Re-evaluate benchmark queries
- Refresh test datasets

### Performance Regression Detection

**Automated Monitoring**:
```bash
# CI benchmark script
#!/bin/bash
uv run python benchmarks/transformation_benchmark.py > results.json

# Compare to baseline
python benchmarks/compare_results.py results.json baseline.json

# Fail CI if regression > 10%
```

**Regression Thresholds**:
- **Transformation**: >10% slowdown triggers investigation
- **End-to-end**: >15% slowdown requires attention
- **Cache performance**: >5% hit rate drop needs review

---

## Limitations & Caveats

### Benchmark Limitations

1. **Synthetic Data**: May not reflect real-world data distributions
2. **Single System**: Results may not generalize to all hardware
3. **Microbenchmarks**: May not capture system-level interactions
4. **Memory Configuration**: fsync=off affects durability but not performance

### Real-World Considerations

**Production Factors Not Benchmarked**:
- Connection pooling overhead
- SSL/TLS encryption
- Load balancer latency
- Multi-tenant isolation
- Background job interference

**Environmental Differences**:
- Cloud vs bare metal performance
- Container orchestration overhead
- Network latency in distributed setups
- Database replication lag

---

## Contributing to Benchmarks

### Adding New Benchmarks

1. **Define objective**: What performance aspect are you measuring?
2. **Establish baseline**: What are you comparing against?
3. **Document methodology**: Hardware, software, procedures
4. **Provide reproducibility**: Scripts and setup instructions
5. **Include statistics**: Mean, variance, confidence intervals

### Benchmark Quality Checklist

- [ ] Clear objective and hypothesis
- [ ] Detailed hardware/software specifications
- [ ] Reproducible setup instructions
- [ ] Statistical analysis of results
- [ ] Discussion of limitations
- [ ] Comparison to relevant baselines

---

*Benchmark Methodology - Ensuring Accurate & Reproducible Performance Measurements*
*Last updated: October 2025*
