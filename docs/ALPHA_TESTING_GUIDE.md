# FraiseQL v2.0.0-alpha.1 Testing Guide

Welcome to the FraiseQL v2 alpha release! This guide helps you effectively test the system and provide valuable feedback.

---

## 🎯 What We Need from Alpha Testers

### Critical Testing Areas (High Priority)

1. **Schema Compilation**
   - [ ] Define schemas in Python, TypeScript, Go, or PHP
   - [ ] Run `fraiseql-cli compile` on your schema
   - [ ] Verify compiled schema matches your expectations
   - [ ] Test with edge cases (nullable fields, complex types, unions)

2. **Query Execution**
   - [ ] Run simple queries (SELECT-like operations)
   - [ ] Test filtering with WHERE operators
   - [ ] Test sorting and pagination
   - [ ] Execute mutations (INSERT/UPDATE operations)

3. **Database Support**
   - [ ] PostgreSQL (primary - most tested)
   - [ ] MySQL (secondary)
   - [ ] SQLite (development/testing)
   - [ ] SQL Server (enterprise)

4. **Authentication & Security**
   - [ ] Test OAuth2/OIDC flows (Google, GitHub, Auth0)
   - [ ] Verify rate limiting is working
   - [ ] Test field-level access control
   - [ ] Validate error messages don't leak sensitive info

### Important Testing Areas (Medium Priority)

1. **Federation** (if you have multiple services)
   - [ ] Setup Apollo Federation
   - [ ] Test entity resolution
   - [ ] Verify SAGA transactions work correctly

2. **Streaming & Performance**
   - [ ] Test Arrow Flight data export (if using analytics)
   - [ ] Stream large result sets with fraiseql-wire
   - [ ] Monitor performance under load
   - [ ] Check memory usage patterns

3. **Integration Features**
   - [ ] Webhooks (Discord, Slack, custom)
   - [ ] Change Data Capture (CDC) events
   - [ ] Caching and query invalidation

4. **Operations**
   - [ ] Deploy to Docker and Kubernetes
   - [ ] Setup monitoring (Prometheus metrics)
   - [ ] Configure structured logging
   - [ ] Test health check endpoints

---

## ⚠️ Known Limitations (Alpha Phase)

### Feature Limitations

**Not Included in Alpha:**

- Subscriptions/real-time queries (planned for v2.1)
- GraphQL directives beyond `@auth` and `@cache` (others planned for v2.1)
- Advanced performance optimizations (deferred to v2.1)
- Oracle database support (no Rust driver available)

**Partially Supported:**

- Language SDKs: Only Python, TypeScript, Go, PHP ready for alpha. Other languages coming in beta/GA.
- Integration providers: 11 webhook providers included; more planned for v2.1

### Performance Notes

The alpha release prioritizes **correctness over optimization**. You may observe:

- **P95 Latency**: ~145ms on typical queries (target is <100ms for GA)
- **Memory Usage**: Reasonable for typical workloads, but not yet micro-optimized
- **Arrow Flight**: Performs well (50x faster than JSON) but schema pre-loading can be optimized

These are **not blocking issues** and won't affect functionality.

### Breaking Changes from v1

FraiseQL v2 is a complete redesign and **not backwards compatible** with v1:

- **Schema format**: Completely different (v1 schema won't work)
- **Configuration**: Now TOML-based instead of environment variables
- **Database conventions**: New naming scheme (tb_*, v_*, fn_*)
- **API**: GraphQL is similar but with new field semantics

**Migration path**: Currently, you'll need to rewrite your schema for v2. A migration guide is coming in beta.

---

## 🚀 Quick Start for Testing

### 1. Install FraiseQL

**Option A: From source**

```bash
git clone https://github.com/fraiseql/fraiseql.git
cd fraiseql
cargo build --release
./target/release/fraiseql-cli --version
```

**Option B: With Docker**

```bash
docker build -t fraiseql:alpha .
docker run fraiseql:alpha fraiseql-cli --version
```

### 2. Define a Test Schema

Create `schema.py`:

```python
from fraiseql import type as fraiseql_type, query as fraiseql_query, schema

@fraiseql_type
class User:
    id: int
    name: str
    email: str | None

@fraiseql_query(sql_source="v_users")
def users(limit: int = 10) -> list[User]:
    pass

schema.export_schema("schema.json")
```

Run: `python schema.py`

### 3. Compile Schema

```bash
fraiseql-cli compile schema.json -o schema.compiled.json
```

### 4. Setup Database

For PostgreSQL, create your views:

```sql
CREATE VIEW v_users AS
SELECT id, name, email FROM tb_user;
```

For other databases, see [Database Schema Conventions](docs/specs/schema-conventions.md).

### 5. Run Server

Create `config.toml`:

```toml
[server]
bind_addr = "0.0.0.0:8080"
database_url = "postgresql://localhost/testdb"

[fraiseql.security]
rate_limiting.enabled = true
```

Start server:

```bash
fraiseql-server -c config.toml --schema schema.compiled.json
```

### 6. Test Queries

```bash
curl -X POST http://localhost:8080/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ users(limit: 5) { id name email } }"}'
```

---

## 🐛 How to Report Issues

### Using GitHub Issues

1. Go to [FraiseQL Issues](https://github.com/fraiseql/fraiseql/issues)
2. Click **New Issue**
3. Use the appropriate template:
   - **Bug Report** — For broken functionality
   - **Feature Request** — For missing features
   - **Documentation** — For unclear docs

### What to Include

**For bugs:**

```
## Description
Brief description of the issue

## Steps to Reproduce

1. Define schema with...
2. Compile with...
3. Run query...

## Expected Behavior
What should happen

## Actual Behavior
What actually happened

## Environment

- FraiseQL version: 2.0.0-alpha.1
- Language: Python / TypeScript / Go / PHP
- Database: PostgreSQL 15 / MySQL 8.0 / etc.
- OS: Linux / macOS / Windows
- Error message (if applicable)
```

**For feature requests:**

```
## Use Case
Why do you need this?

## Proposed Solution
How should this work?

## Current Workaround
Are you working around this now?
```

### Tag Your Issue

Please add the **`alpha`** label to alpha-specific issues. Other useful labels:

- `bug` — Something is broken
- `documentation` — Docs need improvement
- `performance` — Performance issue
- `security` — Security concern
- `question` — Need clarification

---

## 📊 Feedback We Want

### Schema & Type System

- [ ] Are type definitions intuitive?
- [ ] Is automatic WHERE operator generation working?
- [ ] Are field scalar types useful?
- [ ] Any missing type features?

### Query Execution

- [ ] Are query results correct?
- [ ] Is filtering working as expected?
- [ ] Pagination behavior correct?
- [ ] Performance acceptable?

### Security

- [ ] OAuth2/OIDC flow smooth?
- [ ] Rate limiting effective?
- [ ] Error messages appropriate (not too detailed)?
- [ ] Field-level auth working?

### Operations

- [ ] Docker setup straightforward?
- [ ] Kubernetes deployment clear?
- [ ] Monitoring metrics useful?
- [ ] Health checks working?

### Documentation

- [ ] Getting started guide clear?
- [ ] Examples helpful?
- [ ] Architecture docs understandable?
- [ ] Missing anything important?

---

## 🔍 Testing Checklist

Use this checklist to guide your testing:

### Basic Functionality

- [ ] Schema compiles without errors
- [ ] Server starts with compiled schema
- [ ] Simple query returns data
- [ ] Filtered queries return correct results
- [ ] Sorting works correctly
- [ ] Pagination works (limit/offset or cursor)

### Edge Cases

- [ ] Nullable fields handled correctly
- [ ] Empty result sets work
- [ ] Large result sets handled
- [ ] Special characters in filters work
- [ ] NULL comparisons work
- [ ] Complex nested queries work

### Security

- [ ] Unauthenticated queries rejected (if auth required)
- [ ] Field-level auth enforced
- [ ] SQL injection attempts rejected
- [ ] Rate limiting kicks in after threshold
- [ ] Audit logs record mutations

### Performance

- [ ] Query latency acceptable
- [ ] Memory usage reasonable
- [ ] Database connections pooled
- [ ] No N+1 queries detected
- [ ] Arrow Flight faster than JSON

### Deployment

- [ ] Docker image builds
- [ ] Docker container runs
- [ ] Kubernetes manifests apply
- [ ] Health checks respond
- [ ] Metrics exported to Prometheus

---

## 💬 Sharing Feedback

### GitHub Discussions

For non-urgent feedback, ideas, and questions:

- [GitHub Discussions](https://github.com/fraiseql/fraiseql/discussions)
- Create new discussion with category (Feedback, Questions, Ideas)

### Direct Communication

For confidential feedback or security issues:

- Email: <team@fraiseql.dev>
- **For security issues**: Please don't open public issues. Email first.

### Community Discord

Discord server coming soon for real-time chat with the team and community.

---

## 📈 Performance Benchmarking

If you're interested in performance testing:

### Running Benchmarks

```bash
# Arrow vs JSON serialization
cargo bench -p fraiseql-arrow

# Query execution performance
cargo bench -p fraiseql-core
```

### What to Measure

- Query latency (P50, P95, P99)
- Throughput (queries/second)
- Memory usage at peak load
- CPU utilization
- Database connection overhead

See [Benchmarking Guide](guides/development/benchmarking.md) for detailed setup.

---

## 🎓 Additional Resources

- **[Main README](../README.md)** — Project overview
- **[Architecture Guide](architecture/)** — System design
- **[Database Conventions](specs/schema-conventions.md)** — Schema naming
- **[Reference API](reference/)** — Complete API reference
- **[Language Examples](guides/language-generators.md)** — Code examples
- **[Troubleshooting](../TROUBLESHOOTING.md)** — Common issues

---

## ✅ Final Checklist Before Reporting

- [ ] Issue is not already reported (search GitHub issues)
- [ ] You're using v2.0.0-alpha.1 (check version)
- [ ] You've included steps to reproduce
- [ ] You've tested with the latest code (pull latest)
- [ ] Environment details are included
- [ ] You've added the `alpha` label

---

## 🙏 Thank You

Thank you for testing FraiseQL v2! Your feedback is crucial for making this the best GraphQL execution engine for relational databases.

**Happy testing!**
