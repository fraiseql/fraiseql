# FraiseQL v2 Completion Plan

**Date**: January 17, 2026
**Current Status**: 85-90% complete
**Objective**: Reach 100% production readiness

---

## Executive Summary

FraiseQL v2 has exceptional implementation depth with 850+ passing tests and ~107,000 lines of code. The remaining work falls into **5 categories**:

1. **Server E2E Integration** - Verify server works end-to-end (CRITICAL)
2. **SDK Distribution** - Package Python/TypeScript for distribution (HIGH)
3. **CLI Fact Table Commands** - Complete database validation stubs (MEDIUM)
4. **Documentation** - User guides and API reference (MEDIUM)
5. **Examples** - Additional schema examples (LOW)

---

## Category 1: Server E2E Integration

**Priority**: CRITICAL
**Effort**: 2-3 days
**Risk**: Medium (infrastructure exists, needs verification)

### Problem Statement

The HTTP server (`fraiseql-server`) has complete infrastructure but 5 E2E tests are ignored because they require a running server. We need to verify the full pipeline works:

```
Client → HTTP Request → GraphQL Handler → Executor → Database → Response
```

### Current State

| Component | Status | Location |
|-----------|--------|----------|
| Server core | ✅ Complete | `crates/fraiseql-server/src/server.rs` |
| GraphQL route | ✅ Complete | `crates/fraiseql-server/src/routes/graphql.rs` |
| Schema loader | ✅ Complete | `crates/fraiseql-server/src/schema/loader.rs` |
| Health endpoint | ✅ Complete | `crates/fraiseql-server/src/routes/health.rs` |
| Metrics endpoint | ✅ Complete | `crates/fraiseql-server/src/routes/metrics.rs` |
| E2E tests | ⚠️ 5 ignored | `crates/fraiseql-server/tests/http_server_e2e_test.rs` |

### Tasks

#### 1.1 Create Test Server Harness

**File**: `crates/fraiseql-server/tests/test_server_harness.rs`

```rust
/// TestServer wraps FraiseQL server for E2E testing
pub struct TestServer {
    handle: JoinHandle<()>,
    port: u16,
    shutdown_tx: oneshot::Sender<()>,
}

impl TestServer {
    /// Start server with example schema on random available port
    pub async fn start() -> Self {
        // 1. Find available port
        // 2. Load examples/basic/schema.compiled.json
        // 3. Start server in background task
        // 4. Wait for health check to pass
        // 5. Return handle
    }

    pub fn base_url(&self) -> String {
        format!("http://localhost:{}", self.port)
    }

    pub async fn shutdown(self) {
        // Send shutdown signal and wait for clean exit
    }
}
```

#### 1.2 Compile Example Schema

**Prerequisite**: Ensure `examples/basic/schema.compiled.json` exists

```bash
# Generate compiled schema from intermediate format
./target/release/fraiseql-cli compile \
  examples/basic/schema.json \
  -o examples/basic/schema.compiled.json
```

#### 1.3 Update E2E Tests to Use Harness

**File**: `crates/fraiseql-server/tests/http_server_e2e_test.rs`

Update each ignored test to:

1. Start TestServer
2. Run test against server
3. Shutdown server

```rust
#[tokio::test]
async fn test_health_endpoint_responds() {
    let server = TestServer::start().await;
    let client = create_test_client();

    let response = client
        .get(format!("{}/health", server.base_url()))
        .send()
        .await
        .expect("Request should succeed");

    assert_eq!(response.status(), StatusCode::OK);
    server.shutdown().await;
}
```

#### 1.4 Verify Schema Loading in Server

**Files to verify**:

- `crates/fraiseql-server/src/main.rs` - Entry point
- `crates/fraiseql-server/src/server.rs` - Server initialization
- `crates/fraiseql-server/src/schema/loader.rs` - Schema loading

Ensure:

- Server correctly loads `CompiledSchema` from file
- Executor is properly initialized with schema
- Routes have access to executor

#### 1.5 Run Full E2E Test Suite

```bash
# Build everything
cargo build --release

# Compile example schema
./target/release/fraiseql-cli compile examples/basic/schema.json -o examples/basic/schema.compiled.json

# Run E2E tests (no longer ignored)
cargo test -p fraiseql-server --test http_server_e2e_test
```

### Acceptance Criteria

- [ ] All 5 previously-ignored E2E tests pass
- [ ] TestServer harness handles startup/shutdown cleanly
- [ ] Server loads compiled schema and executes queries
- [ ] Health, metrics, and GraphQL endpoints work
- [ ] No memory leaks or resource issues

---

## Category 2: SDK Distribution

**Priority**: HIGH
**Effort**: 3-5 days total (Python + TypeScript)
**Risk**: Low (code complete, just packaging)

### 2.1 Python SDK (fraiseql-python)

**Current State**: ✅ Fully functional code, ⚠️ Not published to PyPI

| Component | Status | File |
|-----------|--------|------|
| Decorators | ✅ Complete | `src/fraiseql/decorators.py` |
| Registry | ✅ Complete | `src/fraiseql/registry.py` |
| Types | ✅ Complete | `src/fraiseql/types.py` |
| Analytics | ✅ Complete | `src/fraiseql/analytics.py` |
| Schema export | ✅ Complete | `src/fraiseql/schema.py` |
| pyproject.toml | ✅ Complete | `pyproject.toml` |
| README | ⚠️ Needs content | `README.md` |
| Tests | ⚠️ Need more | `tests/` |

#### Tasks

##### 2.1.1 Create Comprehensive README

**File**: `fraiseql-python/README.md`

```markdown
# FraiseQL Python SDK

Schema authoring library for FraiseQL - the compiled GraphQL execution engine.

## Installation

```bash
pip install fraiseql
```

## Quick Start

```python
import fraiseql

@fraiseql.type
class User:
    """A user in the system."""
    id: int
    name: str
    email: str | None

@fraiseql.query(sql_source="v_users")
def users(limit: int = 10) -> list[User]:
    """Get all users."""
    pass

# Export schema
fraiseql.export_schema("schema.json")
```

## Features

- Type-safe schema definition with Python type hints
- GraphQL types via `@fraiseql.type`
- Queries via `@fraiseql.query`
- Mutations via `@fraiseql.mutation`
- Analytics support via `@fraiseql.fact_table` and `@fraiseql.aggregate_query`

## Documentation

See [FraiseQL Documentation](https://fraiseql.dev/docs)

```

##### 2.1.2 Add Unit Tests
**File**: `fraiseql-python/tests/test_decorators.py`

```python
import pytest
import fraiseql
from fraiseql.registry import SchemaRegistry

def setup_function():
    """Clear registry before each test."""
    SchemaRegistry.clear()

def test_type_decorator():
    @fraiseql.type
    class User:
        id: int
        name: str

    schema = SchemaRegistry.export()
    assert len(schema["types"]) == 1
    assert schema["types"][0]["name"] == "User"
    assert len(schema["types"][0]["fields"]) == 2

def test_query_decorator():
    @fraiseql.query(sql_source="v_users")
    def users(limit: int = 10) -> list[str]:
        pass

    schema = SchemaRegistry.export()
    assert len(schema["queries"]) == 1
    assert schema["queries"][0]["name"] == "users"
    assert schema["queries"][0]["sql_source"] == "v_users"

def test_nullable_fields():
    @fraiseql.type
    class Post:
        id: int
        content: str | None

    schema = SchemaRegistry.export()
    fields = schema["types"][0]["fields"]
    assert fields[0]["nullable"] is False  # id
    assert fields[1]["nullable"] is True   # content
```

##### 2.1.3 Add Integration Test

**File**: `fraiseql-python/tests/test_schema_export.py`

```python
import json
import tempfile
from pathlib import Path

import fraiseql

def test_full_schema_export():
    """Test complete schema export workflow."""

    @fraiseql.type
    class User:
        id: int
        name: str
        email: str

    @fraiseql.query(sql_source="v_users")
    def users(limit: int = 100) -> list[User]:
        pass

    with tempfile.NamedTemporaryFile(suffix=".json", delete=False) as f:
        fraiseql.export_schema(f.name)

        schema = json.loads(Path(f.name).read_text())

        assert "types" in schema
        assert "queries" in schema
        assert len(schema["types"]) == 1
        assert len(schema["queries"]) == 1
```

##### 2.1.4 Set Up CI/CD for PyPI

**File**: `.github/workflows/python-publish.yml`

```yaml
name: Publish Python Package

on:
  release:
    types: [published]

jobs:
  publish:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Set up Python
        uses: actions/setup-python@v5
        with:
          python-version: '3.11'

      - name: Install dependencies
        run: |
          cd fraiseql-python
          pip install hatch

      - name: Build package
        run: |
          cd fraiseql-python
          hatch build

      - name: Publish to PyPI
        env:
          HATCH_INDEX_USER: __token__
          HATCH_INDEX_AUTH: ${{ secrets.PYPI_TOKEN }}
        run: |
          cd fraiseql-python
          hatch publish
```

##### 2.1.5 Test Local Installation

```bash
cd fraiseql-python
pip install -e .
python -c "import fraiseql; print(fraiseql.__version__)"
```

### 2.2 TypeScript SDK (fraiseql-typescript)

**Current State**: ✅ Fully functional code, ⚠️ Not published to npm

| Component | Status | File |
|-----------|--------|------|
| Decorators | ✅ Complete | `src/decorators.ts` |
| Registry | ✅ Complete | `src/registry.ts` |
| Types | ✅ Complete | `src/types.ts` |
| Analytics | ✅ Complete | `src/analytics.ts` |
| Schema export | ✅ Complete | `src/schema.ts` |
| package.json | ✅ Complete | `package.json` |
| README | ⚠️ Needs content | `README.md` |
| Tests | ⚠️ Need more | `tests/` |

#### Tasks

##### 2.2.1 Create Comprehensive README

**File**: `fraiseql-typescript/README.md`

```markdown
# FraiseQL TypeScript SDK

Schema authoring library for FraiseQL - the compiled GraphQL execution engine.

## Installation

```bash
npm install fraiseql
# or
pnpm add fraiseql
```

## Quick Start

```typescript
import { type, query, exportSchema } from 'fraiseql';

@type()
class User {
  id!: number;
  name!: string;
  email?: string;
}

@query({ sqlSource: 'v_users' })
function users(limit: number = 10): User[] {
  return [];
}

// Export schema
exportSchema('schema.json');
```

## Features

- Type-safe schema definition with TypeScript decorators
- GraphQL types via `@type()`
- Queries via `@query()`
- Mutations via `@mutation()`
- Analytics support via `@factTable()` and `@aggregateQuery()`

## Documentation

See [FraiseQL Documentation](https://fraiseql.dev/docs)

```

##### 2.2.2 Add Unit Tests
**File**: `fraiseql-typescript/tests/decorators.test.ts`

```typescript
import { type, query, mutation, SchemaRegistry } from '../src';

beforeEach(() => {
  SchemaRegistry.clear();
});

describe('@type decorator', () => {
  it('registers a type with fields', () => {
    @type()
    class User {
      id!: number;
      name!: string;
    }

    const schema = SchemaRegistry.export();
    expect(schema.types).toHaveLength(1);
    expect(schema.types[0].name).toBe('User');
  });
});

describe('@query decorator', () => {
  it('registers a query with sql source', () => {
    @query({ sqlSource: 'v_users' })
    function users(): string[] {
      return [];
    }

    const schema = SchemaRegistry.export();
    expect(schema.queries).toHaveLength(1);
    expect(schema.queries[0].sqlSource).toBe('v_users');
  });
});
```

##### 2.2.3 Set Up CI/CD for npm

**File**: `.github/workflows/npm-publish.yml`

```yaml
name: Publish npm Package

on:
  release:
    types: [published]

jobs:
  publish:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Set up Node.js
        uses: actions/setup-node@v4
        with:
          node-version: '20'
          registry-url: 'https://registry.npmjs.org'

      - name: Install dependencies
        run: |
          cd fraiseql-typescript
          npm ci

      - name: Build
        run: |
          cd fraiseql-typescript
          npm run build

      - name: Publish to npm
        env:
          NODE_AUTH_TOKEN: ${{ secrets.NPM_TOKEN }}
        run: |
          cd fraiseql-typescript
          npm publish
```

##### 2.2.4 Test Local Build

```bash
cd fraiseql-typescript
npm install
npm run build
npm run test
```

### Acceptance Criteria

- [ ] Python SDK installable via `pip install fraiseql`
- [ ] TypeScript SDK installable via `npm install fraiseql`
- [ ] Both SDKs have README with usage examples
- [ ] Both SDKs have unit tests passing
- [ ] CI/CD pipelines configured for publishing

---

## Category 3: CLI Fact Table Commands

**Priority**: MEDIUM
**Effort**: 2-3 days
**Risk**: Low (stubs exist, need database integration)

### Current State

The CLI has fact table commands with stub implementations:

| Command | Status | File |
|---------|--------|------|
| `validate facts` | ⚠️ Stub | `crates/fraiseql-cli/src/commands/validate_facts.rs` |
| `introspect facts` | ⚠️ Stub | `crates/fraiseql-cli/src/commands/introspect_facts.rs` |

### Tasks

#### 3.1 Implement Database Connection in CLI

**File**: `crates/fraiseql-cli/src/db.rs`

```rust
use fraiseql_core::db::postgres::PostgresAdapter;
use fraiseql_core::db::traits::DatabaseAdapter;

pub async fn create_adapter(database_url: &str) -> Result<Box<dyn DatabaseAdapter>> {
    // Parse URL scheme to determine adapter type
    if database_url.starts_with("postgres") {
        Ok(Box::new(PostgresAdapter::new(database_url).await?))
    } else if database_url.starts_with("mysql") {
        Ok(Box::new(MySqlAdapter::new(database_url).await?))
    } else {
        anyhow::bail!("Unsupported database URL: {}", database_url)
    }
}
```

#### 3.2 Complete validate_facts Command

**File**: `crates/fraiseql-cli/src/commands/validate_facts.rs`

```rust
pub async fn run(schema_path: &Path, database_url: &str) -> Result<()> {
    // 1. Load schema
    let schema_str = fs::read_to_string(schema_path)?;
    let parser = SchemaParser::new();
    let ir: AuthoringIR = parser.parse(&schema_str)?;

    // 2. Connect to database
    let adapter = create_adapter(database_url).await?;

    // 3. List actual fact tables in database
    let detector = FactTableDetector::new(&adapter);
    let actual_tables = detector.list_fact_tables().await?;

    // 4. Validate each declared fact table
    let mut issues = Vec::new();
    for (table_name, declared) in &ir.fact_tables {
        if !actual_tables.contains(table_name) {
            issues.push(ValidationIssue::error(
                table_name.clone(),
                "Table does not exist in database".to_string(),
            ));
            continue;
        }

        // Introspect actual structure
        let actual = detector.introspect(table_name).await?;

        // Compare declared vs actual
        if let Err(e) = validate_metadata_match(declared, &actual) {
            issues.push(ValidationIssue::error(table_name.clone(), e));
        }
    }

    // 5. Warn about undeclared tf_* tables
    for table_name in &actual_tables {
        if !ir.fact_tables.contains_key(table_name) {
            issues.push(ValidationIssue::warning(
                table_name.clone(),
                "Table exists but not declared in schema".to_string(),
            ));
        }
    }

    // 6. Report results
    report_issues(&issues)
}
```

#### 3.3 Complete introspect_facts Command

**File**: `crates/fraiseql-cli/src/commands/introspect_facts.rs`

```rust
pub async fn run(database_url: &str, format: OutputFormat) -> Result<()> {
    // 1. Connect to database
    let adapter = create_adapter(database_url).await?;

    // 2. List fact tables
    let detector = FactTableDetector::new(&adapter);
    let tables = detector.list_fact_tables().await?;

    // 3. Introspect each table
    let mut metadata = Vec::new();
    for table_name in tables {
        let meta = detector.introspect(&table_name).await?;
        metadata.push(meta);
    }

    // 4. Format output
    match format {
        OutputFormat::Python => format_as_python(&metadata),
        OutputFormat::Json => format_as_json(&metadata),
    }
}
```

### Acceptance Criteria

- [ ] `fraiseql validate facts --schema schema.json --database postgres://...` works
- [ ] `fraiseql introspect facts --database postgres://... --format python` works
- [ ] Commands handle connection errors gracefully
- [ ] Output matches documented format

---

## Category 4: Documentation

**Priority**: MEDIUM
**Effort**: 5-7 days
**Risk**: Low (no code changes)

### Tasks

#### 4.1 User Documentation

| Document | Location | Content |
|----------|----------|---------|
| Getting Started | `docs/getting-started.md` | Installation, first schema, first query |
| Schema Guide | `docs/schema-guide.md` | Types, queries, mutations, analytics |
| CLI Reference | `docs/cli-reference.md` | All commands with examples |
| Deployment | `docs/deployment.md` | Production deployment guide |
| Migration | `docs/migration-v1-to-v2.md` | Breaking changes, upgrade path |

#### 4.2 API Reference

Generate rustdoc with:

```bash
cargo doc --no-deps --open
```

Create landing page at `docs/api/index.md` linking to:

- fraiseql-core API
- fraiseql-server API
- fraiseql-cli API

#### 4.3 Architecture Overview

**File**: `docs/architecture.md`

Cover:

- Compilation pipeline (Schema → IR → SQL → Execution)
- Database abstraction layer
- Cache coherency model
- Security middleware stack

### Acceptance Criteria

- [ ] New users can get started in < 15 minutes
- [ ] All CLI commands documented with examples
- [ ] API reference generated and accessible
- [ ] Architecture diagram explains the system

---

## Category 5: Example Schemas

**Priority**: LOW
**Effort**: 3-5 days
**Risk**: Low (additive)

### Tasks

#### 5.1 E-commerce Example

**Location**: `examples/ecommerce/`

```
examples/ecommerce/
├── schema.py           # Python schema definition
├── schema.json         # Generated intermediate schema
├── schema.compiled.json # Compiled schema
├── setup.sql           # Database setup
└── README.md           # Example documentation
```

Types: Product, Order, OrderItem, Customer, Inventory

#### 5.2 SaaS Multi-tenant Example

**Location**: `examples/saas-multitenant/`

Demonstrates:

- Tenant isolation
- Row-level security
- Shared schema

#### 5.3 Analytics Dashboard Example

**Location**: `examples/analytics/`

Demonstrates:

- Fact tables
- Aggregate queries
- Temporal bucketing
- Window functions

### Acceptance Criteria

- [ ] Each example has working schema
- [ ] Each example has setup instructions
- [ ] Each example demonstrates specific use case

---

## Implementation Order

### Phase A: Critical Path (Week 1)

1. **Server E2E Integration** (2-3 days)
   - Create test harness
   - Verify server pipeline
   - Fix any issues found

### Phase B: CLI Completion (Week 1-2)

2. **CLI Fact Table Commands** (2-3 days)
   - Database connection
   - Validation logic
   - Introspection output

### Phase C: Documentation (Week 2-3)

3. **Documentation** (5-7 days)
   - Getting started
   - CLI reference
   - API docs

### Phase D: Examples (Week 3)

4. **Example Schemas** (3-5 days)
   - E-commerce
   - SaaS multi-tenant
   - Analytics

### Phase E: Distribution (Week 4)

5. **Python SDK Distribution** (2 days)
   - README, tests, CI/CD

6. **TypeScript SDK Distribution** (2 days)
   - README, tests, CI/CD

---

## Risk Mitigation

| Risk | Impact | Mitigation |
|------|--------|------------|
| Server E2E fails | HIGH | Fix is localized to server crate |
| SDK tests fail | MEDIUM | Code already works, just need test fixes |
| Fact table introspection | LOW | Feature is optional, stubs work |
| Documentation gaps | LOW | Code is self-documenting |

---

## Success Metrics

| Metric | Target | Current |
|--------|--------|---------|
| Test pass rate | 100% | 100% |
| E2E tests passing | 100% | ~85% (5 ignored) |
| SDK distribution | Both published | 0% |
| Documentation coverage | 80% | ~40% |
| Example schemas | 4+ | 1 |

---

## Summary

**Total estimated effort**: 15-22 days

| Category | Effort | Priority |
|----------|--------|----------|
| Server E2E | 2-3 days | CRITICAL |
| SDK Distribution | 3-5 days | HIGH |
| CLI Fact Tables | 2-3 days | MEDIUM |
| Documentation | 5-7 days | MEDIUM |
| Examples | 3-5 days | LOW |

**Recommended approach**: Complete Categories 1-3 first (8-11 days) for functional completeness, then add documentation and examples.
