# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |

## Reporting a Vulnerability

Please report security vulnerabilities to security@fraiseql.com or through GitHub's security advisory feature.

## Scope

This security policy applies to the FraiseQL library itself (`src/fraiseql`). 

### Out of Scope

The following components are **not** covered by this security policy as they are development/testing tools only:

- `/benchmarks/` - Performance benchmarking tools (not part of the distributed package)
- `/java-benchmark/` - Java comparison benchmarks (separate project for testing only)
- `/examples/` - Example applications (demonstration purposes only)
- `/tests/` - Test suite (development only)

These directories contain dependencies that may have known vulnerabilities but are:
- Never included in the PyPI package
- Used only for development and benchmarking
- Not executed in production environments

## Security Best Practices

When using FraiseQL in production:

1. Always use the latest version from PyPI
2. Review and apply security headers as documented
3. Enable rate limiting for public APIs
4. Use environment variables for sensitive configuration
5. Follow the production deployment guide

## Dependency Management

Production dependencies are carefully maintained and regularly updated. Development and benchmark dependencies are updated less frequently as they don't affect production deployments.