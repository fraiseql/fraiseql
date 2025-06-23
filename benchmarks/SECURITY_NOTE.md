# Security Note for Benchmark Directory

This directory contains benchmarking tools and comparison frameworks that are **not part of the FraiseQL library distribution**.

## Important Information

- These benchmarks are for development and performance testing only
- Dependencies in this directory may have known vulnerabilities
- These tools are never included in the PyPI package
- They should only be run in isolated development environments

## Why We Don't Update These Dependencies

1. **Historical Accuracy**: Some benchmarks compare against older versions of frameworks to show performance improvements over time
2. **Isolation**: These tools run in isolated environments and don't affect production systems
3. **Not Distributed**: The benchmark code is not included in the packaged library

## Running Benchmarks Safely

If you need to run these benchmarks:

1. Use a isolated virtual environment or container
2. Never run on production systems
3. Be aware that dependencies may have vulnerabilities
4. Consider updating dependencies locally if security is a concern

## Reporting Issues

Security issues in the main FraiseQL library should be reported according to our security policy. Issues specific to benchmark dependencies can be discussed in GitHub issues but are not considered security vulnerabilities of the FraiseQL project.
