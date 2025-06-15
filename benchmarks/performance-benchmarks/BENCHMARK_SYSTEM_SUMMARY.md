# FraiseQL Performance Benchmark System

## Overview

This directory contains a comprehensive performance benchmarking system for comparing FraiseQL with other GraphQL frameworks (primarily Strawberry GraphQL). The system has been designed with adaptive scaling and unified socket-based architecture for accurate performance measurement.

## Key Components

### 1. **Adaptive Profile Detection** (`detect_benchmark_profile.py`)
- Automatically detects system capabilities (CPU, memory, cache)
- Calculates a performance score based on hardware
- Recommends appropriate data scale profiles:
  - **Minimal**: 100 users, 500 products, 200 orders
  - **Small**: 1K users, 5K products, 2K orders
  - **Medium**: 10K users, 50K products, 20K orders
  - **Large**: 50K users, 200K products, 100K orders
  - **XLarge**: 100K users, 1M products, 5M orders

### 2. **Adaptive Data Generation** (`create_adaptive_seed.sh`)
- Generates SQL seed data based on detected profile
- Uses environment variables for data scale
- Optimized batch processing for large datasets
- Progress tracking during generation

### 3. **Unified Socket Architecture** (`unified-socket/`)
- PostgreSQL and application in same container
- Connected via Unix socket (no network overhead)
- Supervisor-managed processes
- Optimized PostgreSQL configuration

### 4. **Benchmark Runner** (`benchmark_runner.py`)
- Comprehensive query testing suite
- Warmup and iteration configuration
- Statistical analysis (mean, median, P95, P99)
- Color-coded performance comparison
- JSON result storage with metadata

## Architecture

### Container Structure
```
[Unified Container]
├── PostgreSQL (via Unix socket)
├── Application (FraiseQL/Strawberry)
└── Supervisor (process manager)
```

### Key Improvements Over Traditional Benchmarks
1. **No Network Overhead**: Unix socket connection eliminates network latency
2. **Adaptive Scaling**: Automatically adjusts to system capabilities
3. **Fixed Query Registration**: Uses `benchmark_app.py` with corrected type registration
4. **Profile-Based Testing**: Adjusts iterations and data volume to system capacity

## Running Benchmarks

### Quick Start
```bash
./run_benchmark.sh
```

This will:
1. Detect your system profile
2. Generate appropriate data scale
3. Build unified containers
4. Run performance tests
5. Generate comparison report

### Manual Profile Override
```bash
export BENCHMARK_PROFILE=small  # or medium, large, etc.
./run_benchmark.sh
```

## Understanding Results

### Metrics Collected
- **Mean Response Time**: Average query execution time
- **P95/P99 Latency**: 95th/99th percentile response times
- **Throughput (RPS)**: Requests per second
- **Error Rate**: Failed request percentage

### Performance Ratios
The benchmark automatically calculates:
- Latency ratio (lower is better for tested framework)
- Throughput ratio (higher is better for tested framework)
- Overall performance summary

## Directory Structure
```
performance-benchmarks/
├── unified-socket/           # Unified container configurations
├── fraiseql/                 # FraiseQL benchmark app
├── strawberry-sqlalchemy/    # Strawberry comparison app
├── shared/                   # Shared resources (DB schema)
├── detect_benchmark_profile.py
├── create_adaptive_seed.sh
├── benchmark_runner.py
└── run_benchmark.sh
```

## Technical Decisions

### Why Unix Sockets?
- Eliminates network stack overhead
- Provides most accurate framework performance comparison
- Standard practice for high-performance database connections

### Why Adaptive Profiles?
- 5M orders would overwhelm laptop systems
- Ensures benchmarks complete in reasonable time
- Allows fair comparison across different hardware
- Focuses on framework efficiency, not hardware limits

### Fixed Issues
- Query type registration bug in FraiseQL (`benchmark_app.py`)
- Adaptive scaling prevents overwhelming systems
- PostgreSQL version detection for container compatibility

## Next Steps

1. **Run the benchmark**: `./run_benchmark.sh`
2. **Review results**: Check generated JSON files
3. **Optimize based on findings**: Focus on bottlenecks identified
4. **Compare across systems**: Run on different hardware profiles

## Notes

- The system detected your i5-6300U as "Medium" profile (10K users, 50K products, 20K orders)
- This is appropriate for a 4-core system with 31GB RAM
- Benchmarks should complete in 5-10 minutes at this scale
