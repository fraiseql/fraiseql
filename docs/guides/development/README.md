<!-- Skip to main content -->
---
title: Development Guide
description: Tools and guides for FraiseQL development and testing.
keywords: ["debugging", "implementation", "best-practices", "deployment", "tutorial"]
tags: ["documentation", "reference"]
---

# Development Guide

Tools and guides for FraiseQL development and testing.

## Getting Set Up

- **[Developer Guide](DEVELOPER_GUIDE.md)** — Development environment setup
- **[Linting](LINTING.md)** — Code quality standards and linting

## Testing

### Test Strategies

- **[Testing Strategy](../testing-strategy.md)** — Complete testing approach
  - Unit testing
  - Integration testing
  - End-to-end testing
  - Performance testing
  - Test data management

### E2E Testing

- **[E2E Testing Guide](e2e-testing.md)** — End-to-end testing with real services
- **[Test Coverage](TEST_COVERAGE.md)** — Measure and improve test coverage

## Performance

### Benchmarking

- **[Benchmarking Guide](benchmarking.md)** — Performance benchmarking with Criterion
  - Set up benchmark infrastructure
  - Run and interpret results
  - Track performance regressions
  - CI/CD integration

### Profiling

- **[Profiling Guide](PROFILING_GUIDE.md)** — Profile and optimize code
  - Identify bottlenecks
  - Flame graphs
  - Memory profiling
  - Database query analysis

## Quick Commands

```bash
<!-- Code example in BASH -->
# Lint code
cargo clippy --all-targets --all-features

# Run tests
cargo test

# Run E2E tests
make e2e-all

# Run benchmarks
bash BENCHMARK_QUICK_START.sh setup
bash BENCHMARK_QUICK_START.sh run-small

# Profile code
cargo flamegraph --bin FraiseQL-server
```text
<!-- Code example in TEXT -->

## Common Tasks

- **Add a new feature** → Start with [Testing Strategy](../testing-strategy.md) (TDD approach)
- **Improve performance** → Use [Benchmarking Guide](benchmarking.md) to measure
- **Debug an issue** → Use [Profiling Guide](PROFILING_GUIDE.md) to find root cause
- **Ensure code quality** → Run [Linting](LINTING.md) before commit

---

**Version**: v2.0.0
**Last Updated**: February 1, 2026
