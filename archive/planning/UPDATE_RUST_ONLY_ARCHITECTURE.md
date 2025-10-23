# Task: Update Documentation for Rust-Only Architecture

**Date Created:** 2025-10-17
**Context:** fraiseql_rs v0.2.0 makes Rust REQUIRED, not optional
**Priority:** HIGH - Major architectural change

---

## Background

With **fraiseql_rs v0.2.0**, FraiseQL has migrated to a **Rust-only pipeline**. This is a fundamental architectural change that affects:

1. **Installation** - Rust is now required
2. **Deployment** - Docker images need Rust toolchain
3. **Architecture** - No more "choose between Python/Rust"
4. **Dependencies** - CamelForge (PostgreSQL function) completely removed
5. **Documentation** - References to "optional Rust" are now wrong

---

## Key Architectural Changes

### BEFORE (v0.11.x and earlier)

**Optional Rust:**
- Python transformation available as fallback
- CamelForge (PostgreSQL function) as alternative
- Users could choose: Python-only, Rust-accelerated, or PL/pgSQL

**Flexible but complex:**
```python
# Could configure transformation backend
config = FraiseQLConfig(
    use_rust=True,  # Optional!
    use_camelforge=False  # Alternative!
)
```

### AFTER (v0.11.5+ with fraiseql_rs v0.2.0)

**Rust Required:**
- fraiseql_rs is a hard dependency
- No Python fallback exists
- CamelForge removed entirely
- Single unified pipeline

**Simpler but inflexible:**
```python
# Rust is always used, no configuration needed
config = FraiseQLConfig(
    db_url="postgresql://..."
    # That's it! Rust pipeline is automatic
)
```

---

## What Needs to Change

### 1. **Installation Documentation**

#### Files to Update:
- `/README.md` - Installation section
- `/INSTALLATION.md` - Full install guide
- `/docs/quickstart.md` - Getting started
- `/pyproject.toml` - Dependencies (already updated)

#### Changes Needed:

**REMOVE these statements:**
- ❌ "Rust is optional for better performance"
- ❌ "Install Rust if you want faster transformation"
- ❌ "FraiseQL works without Rust (Python fallback)"

**ADD these statements:**
- ✅ "Rust is required (via fraiseql_rs)"
- ✅ "Install Rust toolchain before installing FraiseQL"
- ✅ "maturin will build fraiseql_rs automatically"

**Example:**

```markdown
## Installation

### Prerequisites

**Required:**
- Python 3.13+
- PostgreSQL 15+
- **Rust 1.70+** (for fraiseql_rs compilation)

**Install Rust:**
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

**Install FraiseQL:**
```bash
pip install fraiseql
# fraiseql_rs will be compiled automatically via maturin
```

### Binary Wheels

Pre-compiled wheels are available for:
- Linux (x86_64, aarch64)
- macOS (x86_64, Apple Silicon)
- Windows (x86_64)

If your platform isn't supported, Rust compilation happens automatically.
```

---

### 2. **Architecture Documentation**

#### Files to Update:
- `/README.md` - Architecture section
- `/docs/core/fraiseql-philosophy.md` - Philosophy
- `/docs/performance/index.md` - Performance guide
- `/RUST_FIRST_PIPELINE.md` - Pipeline documentation

#### Remove These Concepts:

**❌ "Optional Rust Acceleration"**
```markdown
# DELETE sections like:
## Optional Rust Acceleration

FraiseQL can optionally use Rust for JSON transformation...
```

**❌ "Choose Your Transformation Backend"**
```markdown
# DELETE sections like:
## Transformation Options

1. **Python** (default) - Pure Python, no compilation
2. **Rust** (recommended) - 10x faster
3. **CamelForge** (PostgreSQL) - Database-side transformation
```

**❌ "CamelForge Integration"**
```markdown
# DELETE all references to CamelForge:
- CREATE FUNCTION camelforge_transform()...
- Installation of PL/pgSQL functions
- CamelForge configuration options
```

#### Add These Concepts:

**✅ "Unified Rust Pipeline"**
```markdown
## Unified Rust Pipeline

FraiseQL uses **fraiseql_rs** (Rust) for all JSON transformation. This provides:
- 7-10x faster transformation than pure Python
- Zero-copy performance
- GIL-free parallelism
- Consistent behavior across all deployments
```

**✅ "Database-First, Rust-Accelerated"**
```markdown
## Architecture

FraiseQL combines:
1. **PostgreSQL** - JSONB views for data caching (`tv_*` tables)
2. **Rust** - Zero-copy JSON transformation (fraiseql_rs)
3. **Python** - FastAPI integration and GraphQL schema

No external dependencies (Redis, Elasticsearch) required.
```

---

### 3. **Configuration Documentation**

#### Files to Update:
- `/docs/core/configuration.md`
- `/docs/reference/config.md`
- `/examples/*/README.md` (configuration sections)

#### Remove These Options:

**❌ Configuration for Python vs Rust:**
```python
# DELETE these config options (they don't exist anymore):
use_rust: bool = True  # Removed
use_python_transformer: bool = False  # Removed
transformation_backend: str = "rust"  # Removed
use_camelforge: bool = False  # Removed
```

#### Add These Clarifications:

**✅ "Rust is Always Used"**
```markdown
## Transformation

FraiseQL uses **fraiseql_rs** automatically for all transformations.
No configuration needed - the Rust pipeline is the only option.

Previously (v0.11.x), you could choose between Python and Rust.
As of v0.11.5+, Rust is the only supported transformation backend.
```

---

### 4. **Deployment Documentation**

#### Files to Update:
- `/deploy/docker/README.md`
- `/docs/production/deployment.md`
- `/.github/workflows/*.yml` (CI/CD)
- `/Dockerfile` (if exists)

#### Changes Needed:

**Dockerfile must include Rust:**

**BEFORE (outdated):**
```dockerfile
FROM python:3.13-slim

# Install Python dependencies only
RUN pip install fraiseql
```

**AFTER (correct):**
```dockerfile
FROM python:3.13-slim

# Install Rust (required for fraiseql_rs)
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

# Install build dependencies
RUN apt-get update && apt-get install -y \
    build-essential \
    libssl-dev \
    pkg-config

# Install FraiseQL (will compile fraiseql_rs)
RUN pip install fraiseql
```

**Better approach - Multi-stage build:**
```dockerfile
# Stage 1: Build fraiseql_rs
FROM rust:1.70 as rust-builder
WORKDIR /build
COPY fraiseql_rs/ ./fraiseql_rs/
RUN cd fraiseql_rs && cargo build --release

# Stage 2: Python application
FROM python:3.13-slim
COPY --from=rust-builder /build/fraiseql_rs/target/release/libfraiseql_rs.so /usr/local/lib/
RUN pip install fraiseql --no-build-isolation
```

---

### 5. **Migration Guides**

#### Files to Update:
- Create: `/docs/migration-guides/v0.11-to-v1.md`
- Update: `/CHANGELOG.md`
- Update: `/FRAISEQL_RS_V0.2_MIGRATION_GUIDE.md`

#### New Migration Guide Content:

```markdown
# Migrating to FraiseQL v1.0 (Rust-Only Architecture)

## Breaking Changes

### Rust is Now Required

**BEFORE (v0.11.4 and earlier):**
- Rust was optional
- Python fallback available
- Could run without compilation

**AFTER (v0.11.5+ / v1.0):**
- Rust is **required**
- No Python fallback
- Must compile fraiseql_rs

### Installation Changes

**Old installation (no Rust needed):**
```bash
pip install fraiseql==0.11.4  # Worked without Rust
```

**New installation (Rust required):**
```bash
# Install Rust first
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Then install FraiseQL
pip install fraiseql  # Compiles fraiseql_rs automatically
```

### CamelForge Removed

**BEFORE:**
```sql
-- CamelForge PostgreSQL function was available
CREATE FUNCTION camelforge_transform(jsonb) RETURNS jsonb...
```

**AFTER:**
```
-- CamelForge removed entirely
-- All transformation happens in Rust
```

### Configuration Cleanup

**Removed config options:**
- `use_rust` - Always true now
- `use_python_transformer` - Doesn't exist
- `use_camelforge` - Removed
- `transformation_backend` - Only Rust

**Your old config:**
```python
config = FraiseQLConfig(
    db_url="...",
    use_rust=True,  # DELETE THIS
    use_camelforge=False  # DELETE THIS
)
```

**New config:**
```python
config = FraiseQLConfig(
    db_url="..."
    # That's it! Rust is automatic
)
```

### Deployment Changes

**Docker:**
- Must include Rust in build stage
- Or use pre-built wheels
- See updated Dockerfile examples

**CI/CD:**
- Install Rust in build pipeline
- Or cache compiled wheels
- Longer build times (first build only)

### Performance Impact

✅ **FASTER:** 7-10x speedup (no Python fallback anymore)
✅ **SIMPLER:** One code path, fewer bugs
✅ **CONSISTENT:** Same behavior everywhere
```

---

### 6. **Dependency Documentation**

#### Files to Update:
- `/README.md` - Dependencies section
- `/pyproject.toml` - Already has `fraiseql-rs` as required
- `/docs/core/dependencies.md`

#### Changes:

**Update dependency table:**

| Dependency | Version | Required | Purpose |
|------------|---------|----------|---------|
| Python | 3.13+ | ✅ Yes | Core runtime |
| PostgreSQL | 15+ | ✅ Yes | Database |
| **Rust** | **1.70+** | **✅ Yes** | **fraiseql_rs compilation** |
| FastAPI | 0.115+ | ✅ Yes | HTTP server |
| psycopg | 3.2+ | ✅ Yes | Database driver |
| **fraiseql_rs** | **0.2.0+** | **✅ Yes** | **JSON transformation** |

**Note changes:**
- Rust moved from "Optional" to "Required"
- fraiseql_rs moved from "Optional" to "Required"
- CamelForge removed entirely

---

### 7. **Performance Documentation**

#### Files to Update:
- `/docs/performance/index.md`
- `/PERFORMANCE_GUIDE.md`
- `/benchmarks/BENCHMARK_RESULTS.md`

#### Changes:

**Remove "Optional Performance" Framing:**

**❌ BEFORE:**
```markdown
## Optional: Enable Rust for Better Performance

For best performance, install Rust...
```

**✅ AFTER:**
```markdown
## Performance Architecture

FraiseQL uses Rust (via fraiseql_rs) for all JSON transformation,
delivering 7-10x faster performance than pure Python alternatives.
```

**Update Benchmark Comparisons:**

**❌ Remove:** "Rust vs Python FraiseQL" comparisons
**✅ Add:** "FraiseQL vs Other GraphQL Frameworks" comparisons

---

### 8. **Example Projects**

#### Files to Update:
- All `/examples/*/README.md` files
- Installation sections in examples

#### Changes:

**In each example's installation section:**

**❌ BEFORE:**
```markdown
## Installation

```bash
pip install fraiseql
# Optional: Install with Rust support
pip install fraiseql[rust]
```
```

**✅ AFTER:**
```markdown
## Installation

**Prerequisites:** Rust 1.70+ (required)

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install FraiseQL
pip install fraiseql
```

Or use pre-built wheels (if available for your platform).
```

---

### 9. **README.md Main Updates**

#### Key Sections to Update:

**1. Badge/Status:**
```markdown
**Status:** v1.0-alpha | Rust Required | PostgreSQL 15+
```

**2. Quick Facts:**
```markdown
- 🚀 **Rust-powered** - 7-10x faster JSON transformation
- 🏗️ **Database-first** - CQRS with PostgreSQL JSONB
- 🎯 **Zero dependencies** - No Redis, Elasticsearch, or external services
- ⚡ **Sub-millisecond** - Materialized views + Rust pipeline
```

**3. Installation (already covered above)**

**4. Project Structure Table:**

| Component | Path | Status | Description | Required |
|-----------|------|--------|-------------|----------|
| **Rust Pipeline** | `fraiseql_rs/` | Stable | Core performance engine | **✅ Required** |
| Main Framework | `src/fraiseql/` | Stable | Python API & FastAPI | ✅ Required |
| Examples | `examples/` | Stable | Reference implementations | ❌ Optional |

**Change:** "Optional" → "Required" for Rust

---

## Search Patterns

Use these to find all references:

```bash
# Find "optional Rust" references
grep -ri "optional.*rust\|rust.*optional" --include="*.md" .

# Find "choose between" references
grep -ri "choose.*python.*rust\|python.*or.*rust" --include="*.md" .

# Find CamelForge references
grep -ri "camelforge\|camel.forge" --include="*.md" --include="*.sql" .

# Find old config options
grep -ri "use_rust\|use_python_transformer\|use_camelforge" --include="*.md" --include="*.py" .

# Find transformation backend references
grep -ri "transformation.*backend\|backend.*transformation" --include="*.md" .
```

---

## Testing the Changes

After updates, verify:

### 1. Fresh Install Works
```bash
# On clean system
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
pip install fraiseql
python -c "import fraiseql_rs; print('✅ Rust working')"
```

### 2. Documentation Consistency
```bash
# No "optional Rust" references
grep -ri "optional.*rust" --include="*.md" . | grep -v "UPDATE_RUST_ONLY"

# No CamelForge references
grep -ri "camelforge" --include="*.md" . | grep -v "archive/"

# No old config options in docs
grep -ri "use_rust\|use_camelforge" --include="*.md" docs/
```

### 3. Examples Still Work
```bash
# Try each example with Rust-only pipeline
cd examples/quickstart
uv run python main.py
```

---

## Migration Impact Assessment

### Users Affected

**✅ New Users:**
- Clear installation path
- No confusion about options
- Single architecture to learn

**⚠️ Existing Users (v0.11.4 and earlier):**
- Must install Rust (may be surprising)
- Configuration cleanup needed
- Docker images need rebuilding
- CI/CD pipelines need updating

### Breaking Changes Summary

1. **Hard dependency on Rust** - Can't install without Rust compiler
2. **CamelForge removed** - PostgreSQL function approach gone
3. **Config options removed** - `use_rust`, `use_camelforge` don't exist
4. **Deployment changes** - Docker/CI must include Rust
5. **No fallback** - Can't use Python-only mode

### Communication Strategy

**Release Notes Must Emphasize:**
```markdown
## 🚨 Breaking Changes in v1.0

### Rust is Now Required

FraiseQL v1.0 requires Rust for compilation. Install Rust before upgrading:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Why This Change?

1. **Simpler architecture** - One transformation path, not three
2. **Better performance** - 7-10x faster than Python
3. **Easier maintenance** - No fallback code to maintain
4. **Consistent behavior** - Same results everywhere

### Migration Path

See [Migration Guide](./docs/migration-guides/v0.11-to-v1.md) for:
- Docker updates
- CI/CD changes
- Configuration cleanup
```

---

## Priority Order

### Phase 1: Core Docs (HIGH PRIORITY)
1. `/README.md` - Main project readme
2. `/INSTALLATION.md` - Install guide
3. `/docs/quickstart.md` - Getting started
4. Create `/docs/migration-guides/v0.11-to-v1.md`

### Phase 2: Architecture Docs (HIGH PRIORITY)
1. `/docs/core/fraiseql-philosophy.md`
2. `/docs/performance/index.md`
3. `/RUST_FIRST_PIPELINE.md`

### Phase 3: Reference Docs (MEDIUM PRIORITY)
1. `/docs/core/configuration.md`
2. `/docs/reference/config.md`
3. `/docs/core/dependencies.md`

### Phase 4: Examples (MEDIUM PRIORITY)
1. All `/examples/*/README.md` files
2. Update installation sections
3. Remove optional flags

### Phase 5: Deployment (HIGH PRIORITY for Production Users)
1. `/deploy/docker/README.md`
2. Update Dockerfiles
3. CI/CD documentation

### Phase 6: Cleanup (LOW PRIORITY)
1. Archive CamelForge docs
2. Remove obsolete config references
3. Clean up old migration guides

---

## Success Criteria

- [ ] No references to "optional Rust" remain
- [ ] All "installation" sections mention Rust requirement
- [ ] CamelForge removed from active docs (archived)
- [ ] Configuration docs don't mention removed options
- [ ] Docker files include Rust or use wheels
- [ ] Migration guide created for v0.11 → v1.0
- [ ] Fresh install works (with Rust)
- [ ] All examples updated

---

## Estimated Effort

- **Core docs:** 2-3 hours
- **Architecture docs:** 1-2 hours
- **Examples:** 2-3 hours (many files)
- **Deployment:** 1-2 hours
- **Testing:** 1 hour
- **Total:** **7-11 hours**

Can be parallelized across multiple agents or sessions.

---

## Related Tasks

This update should be done **together with**:
1. [UPDATE_PERFORMANCE_CLAIMS.md](./UPDATE_PERFORMANCE_CLAIMS.md) - Update 10-80x → 7-10x
2. Phase 2 completion - Benchmark documentation
3. v1.0-alpha release preparation

---

**Ready to proceed?**

Start with Phase 1 (Core Docs), then move through the priority order.
Each phase can be done independently.
