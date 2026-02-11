# FraiseQL Code Cleanliness Report

**Date:** 2026-02-11  
**Scope:** Full codebase audit  
**Codebase Size:** ~322k LOC (Rust), ~319k LOC (Python)

---

## Executive Summary

FraiseQL v2 demonstrates **strong architectural foundations** with clean separation between authoring (Python/TypeScript), compilation (Rust CLI), and runtime (Rust server). The trait-based design, feature-gated modularity, and zero-unsafe-code policy are exemplary.

**Overall Grade: B+**

However, several cleanliness issues have accumulated that should be addressed to maintain long-term code quality and developer velocity.

---

## 1. Critical Issues (Address Immediately)

### 1.1 Excessive Clippy Allow Attributes

**Issue:** 17 Rust source files contain `#![allow(...)]` attributes, with `lib.rs` files having 44-45 allows each.

**Impact:**
- Technical debt accumulation
- Inconsistent code quality standards
- Reduced compiler assistance

**Files Affected:**
- `crates/fraiseql-core/src/lib.rs` (45 allows)
- `crates/fraiseql-server/src/lib.rs` (44 allows)
- 15 additional test/source files

**Recommendation:**
```rust
// Before (in lib.rs):
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]
// ... 43 more lines

// After: Centralize in workspace Cargo.toml
[workspace.lints.clippy]
module_name_repetitions = "allow"
must_use_candidate = "allow"
```

**Action:** Follow existing `CLIPPY_FIXES_PLAN.md` to systematically remove allows.

---

### 1.2 Development Artifacts in Repository

**Issue:** 7 development directories contain planning files, phases, and temporary documentation.

**Directories:**
- `.phases/` - Development phase planning
- `docs/internal/.claude/` - AI assistant context and plans
- `docs/internal/dev/` - Release planning and architecture docs

**Impact:**
- Repository bloat (documentation files in root)
- Confusion for new contributors
- Outdated planning documents

**Recommendation:**
Move to either:
1. Separate `fraiseql-internal` repository
2. `docs/internal/` with clear README
3. GitHub Wiki for planning documents

Keep in repository root ONLY:
- `README.md`
- `CONTRIBUTING.md`
- `LICENSE`
- `CODE_OF_CONDUCT.md`

---

## 2. High Priority (Address This Quarter)

### 2.1 Module Organization Inconsistency

**Issue:** `fraiseql-server` mixes top-level files with directories inconsistently.

**Current Structure:**
```
fraiseql-server/src/
├── server.rs              # Top-level
├── logging.rs             # Top-level (observability concern)
├── metrics_server.rs      # Top-level (observability concern)
├── tracing_server.rs      # Top-level (observability concern)
├── routes/                # Directory
├── middleware/            # Directory
├── auth/                  # Directory
└── encryption/            # Directory
```

**Recommended Structure:**
```
fraiseql-server/src/
├── server.rs
├── routes/
├── middleware/
├── auth/
├── encryption/
└── observability/         # Consolidated
    ├── logging.rs
    ├── metrics.rs
    └── tracing.rs
```

---

### 2.2 Duplicate Test Utilities

**Issue:** Multiple `common/mod.rs` files with similar fixture patterns.

**Locations:**
- `/tests/common/mod.rs`
- `/crates/fraiseql-server/tests/common/mod.rs`
- `/crates/fraiseql-core/tests/common/`

**Recommendation:**
Create `crates/fraiseql-test-utils` crate with:
```rust
pub mod fixtures;
pub mod assertions;
pub mod mock_adapters;
#[cfg(feature = "postgres")]
pub mod pg_test_container;
```

---

### 2.3 Clippy Warnings in CI

**Current State:** 6+ warnings in fraiseql-server tests:
- `len_zero` (use `!is_empty()`)
- `inefficient_to_string` on `&&str`
- `unnecessary_sort_by`
- `useless_vec`

**Fix Commands:**
```bash
# Auto-fix most issues
cargo clippy --all-targets --all-features --fix --allow-dirty

# Remaining manual fixes
# Review and fix: crates/fraiseql-server/src/encryption/query_builder_integration_tests.rs
# Review and fix: crates/fraiseql-server/src/encryption/rotation_api_tests.rs
# Review and fix: crates/fraiseql-server/src/secrets/schema_tests.rs
```

---

## 3. Medium Priority (Address This Release Cycle)

### 3.1 TODO/FIXME Comments in Python Code

**Count:** 19 files contain TODO/FIXME/XXX/HACK markers

**Categories:**
- Tests: `test_observers.py`, `test_schema_introspection_security.py`
- Examples: `app.py`, `models.py` in examples/
- Integration: `test_toml_workflow.py`, `test_where_clause_bug.py`

**Recommendation:**
```python
# Instead of:
# TODO: Fix this later

# Use:
# FIXME(author, date): Specific issue description
# Related: GitHub issue #123
```

**Action:** Create GitHub issues for each TODO and reference them.

---

### 3.2 Python Type Completeness

**Issue:** 43% of Python functions lack complete type annotations.

**Examples from decorators.py:**
```python
# Missing return types
def type_decorator(cls):
    ...

# Missing parameter types  
def query(func):
    ...
```

**Recommendation:**
Enable stricter mypy configuration:
```toml
[tool.mypy]
python_version = "3.10"
warn_return_any = true
warn_unused_ignores = true
disallow_untyped_defs = true
disallow_incomplete_defs = true
check_untyped_defs = true
```

---

### 3.3 Feature Flag Complexity

**Issue:** 12+ feature flags across crates create combinatorial explosion.

**Current Features:**
- Database: `postgres`, `mysql`, `sqlite`, `sqlserver`
- Optional: `observers`, `arrow`, `wire-backend`, `kafka`, `vault`
- Testing: `testing`, `benchmark`

**Recommendation:**
1. Document feature combinations matrix in `FEATURES.md`
2. Create CI jobs for common combinations:
   - `default`
   - `full` (all features)
   - `minimal` (postgres only)
   - `enterprise` (postgres + observers + encryption)

---

## 4. Low Priority (Address When Convenient)

### 4.1 Import Organization

**Issue:** Import groups are sometimes inconsistent.

**Current:**
```python
from fraiseql.registry import SchemaRegistry
from fraiseql.scope import validate_scope
from fraiseql.types import extract_field_info, extract_function_signature

from dataclasses import dataclass
from enum import Enum as PythonEnum
```

**Standard (PEP 8):**
```python
# Standard library
from dataclasses import dataclass
from enum import Enum as PythonEnum
from types import FunctionType
from typing import TYPE_CHECKING, Any, Generic, TypeVar

# Third-party (none in this case)

# Local
from fraiseql.registry import SchemaRegistry
from fraiseql.scope import validate_scope
from fraiseql.types import extract_field_info, extract_function_signature
```

---

### 4.2 Documentation Completeness

**Gap Areas:**
- Complex filtering scenarios in examples
- Federation setup walkthrough
- Observer configuration patterns
- Security configuration reference

**Recommendation:**
Add to `examples/` directory:
```
examples/
├── basic/
├── federation/
├── observers/
├── security/
└── advanced_filters/
```

---

### 4.3 Configuration Flow Simplification

**Current Flow:**
```
fraiseql.toml → JSON (compile) → Server (runtime)
     ↓              ↓                  ↓
  Manual      Automated          Env overrides
```

**Issue:** Multi-stage flow is powerful but complex.

**Recommendation:**
Add validation command:
```bash
fraiseql-cli validate-config fraiseql.toml
# Shows: ✓ Valid, warnings about missing env vars
```

---

## 5. Positive Patterns to Maintain

These practices should be documented and enforced:

1. **Zero Unsafe Code** - `#![forbid(unsafe_code)]` in all crates
2. **Trait-Based Abstraction** - Database adapters behind traits
3. **Compiled Schema Artifact** - Single deployable unit
4. **Feature-Gated Modularity** - Users pay for what they use
5. **Comprehensive Testing** - 2,400+ tests across all layers
6. **No Python FFI at Runtime** - Clean separation of concerns

---

## 6. Action Plan

### Week 1: Quick Wins
- [ ] Run `cargo clippy --fix` and commit fixes
- [x] Organize `.claude/` and `dev/` into `docs/internal/`
- [ ] Create GitHub issues for all TODO/FIXME comments

### Month 1: Structural Improvements  
- [ ] Centralize clippy allows in workspace `Cargo.toml`
- [ ] Create `fraiseql-test-utils` crate
- [ ] Reorganize `fraiseql-server/src/` observability modules

### Quarter 1: Quality Enforcement
- [ ] Enable strict mypy for Python
- [ ] Add CI job for feature combination testing
- [ ] Complete CLIPPY_FIXES_PLAN.md items
- [ ] Write comprehensive examples for gaps

---

## Metrics Dashboard

Track these metrics over time:

| Metric | Current | Target |
|--------|---------|--------|
| Rust clippy allows | 17 files | 0 files |
| Clippy warnings | 6+ | 0 |
| Python TODO/FIXME | 19 files | 0 files |
| Test utility duplication | 3 locations | 1 crate |
| Doc coverage (Rust) | ~75% | 90% |
| Type coverage (Python) | ~57% | 90% |

---

## Conclusion

FraiseQL has a **solid architectural foundation** but needs **cleanup of accumulated technical debt**. The issues identified are typical for a rapidly-evolving codebase and can be addressed systematically without disrupting development.

**Priority:** Focus on clippy cleanup and development artifact organization first for immediate impact.

---
---

# Extended Analysis (2026-02-11)

**Scope:** Deep-dive audit of Rust quality, Python quality, test coverage, CI health, and cross-cutting concerns on the `release/v2.0.0-alpha.3` branch.

**Revised Grade: B-** (downgraded from B+ due to broken imports and build failures)

---

## 7. BLOCKING: Build Failures and Broken Imports

These issues prevent compilation (Rust) and import (Python) entirely. They must be resolved before any other work.

### 7.1 Rust Compiler Errors (4 locations)

**7.1.1 — Missing method `execute_with_security`**
- **File:** `crates/fraiseql-server/src/arrow/executor_wrapper.rs:41`
- **Error:** `no method named 'execute_with_security' found for struct Arc<Executor<A>>`
- **Fix:** The actual method is `execute_with_scopes` (compiler suggestion). Additionally, line 40 and 43 have type inference failures (`E0282`) — the closure parameter needs an explicit type annotation.

**7.1.2 — Missing field `introspection_enabled` in `ServerConfig`**
- **File:** `crates/fraiseql-server/src/routes/graphql.rs` (lines 572, 596, 621, 629)
- **Error:** Four test functions construct `ServerConfig` without the required `introspection_enabled` field added to the struct.
- **Tests affected:** `test_sanitized_config_from_server_config`, `test_sanitized_config_indicates_tls_without_exposing_keys`, `test_sanitized_config_redaction`, and one additional test.

**7.1.3 — Unused imports and dead code in executor**
- **File:** `crates/fraiseql-core/src/runtime/executor.rs`
  - Line 20: unused import `ExecutionContext`
  - Line 291: unused variable `user_scopes` (parameter accepted but never read)
  - Lines 381, 431: dead methods `apply_field_rbac_filtering` and `execute_regular_query_with_security`

### 7.2 Python Package Cannot Be Imported (FATAL)

The `fraiseql` package fails to import due to references to deleted modules:

| Deleted Module | Import Location | Impact |
|----------------|----------------|--------|
| `src/fraiseql/cqrs/executor.py` | `src/fraiseql/__init__.py:7` imports `CQRSExecutor` | **Blocks all `import fraiseql`** |
| `src/fraiseql/execution/mode_selector.py` | `src/fraiseql/execution/__init__.py:3` | Breaks execution subpackage |
| `src/fraiseql/execution/unified_executor.py` | `src/fraiseql/execution/__init__.py:4` | Breaks execution subpackage |
| `src/fraiseql/cli/commands/turbo.py` | `src/fraiseql/cli/commands/__init__.py:11` | Breaks CLI |
| `src/fraiseql/fastapi/*` (entire directory) | 35+ test files, 10+ example files | Breaks tests & examples |

**Cascade effect:** Since `__init__.py` imports `CQRSExecutor` at package level, _nothing_ in the Python codebase can be imported. Every test, every example, every CLI command fails.

### 7.3 Async Property Antipattern (Runtime Crash)

- **File:** `src/fraiseql/auth/auth0.py:87-92`
- **Code:** `@property` combined with `async def http_client(self)` — Python properties cannot be async.
- **Line 245** then does `client = await self.http_client`, which will raise `TypeError: object is not awaitable`.
- **Impact:** Auth0Provider is unusable at runtime.

---

## 8. Version Mismatches Across the Project

The version numbers are inconsistent across four systems:

| Source | Version | Expected |
|--------|---------|----------|
| Branch name | `release/v2.0.0-alpha.3` | — |
| Rust workspace (`Cargo.toml`) | `2.0.0-alpha.3` | Matches |
| CHANGELOG.md | `[2.0.0-alpha.3]` | Matches |
| Python main (`pyproject.toml`) | **`1.9.17`** | Should be `2.0.0a3` |
| Python wheel (`fraiseql-python/pyproject.toml`) | `2.0.0a3` | Matches |
| Dockerfile label | **`2.1.0`** | Should be `2.0.0-alpha.3` |
| `fraiseql-wire` crate | **`0.1.1`** (hardcoded) | Should use workspace version |

The Python main package version (`1.9.17`) and Dockerfile label (`2.1.0`) are both wrong for this release.

Additionally, `pyproject.toml` requires `python >= 3.13, < 3.14` but `fraiseql-python/uv.lock` declares `requires-python = ">=3.10"`.

---

## 9. Rust Code Quality (Beyond Existing Report)

### 9.1 Excessive `#[allow(dead_code)]` — 70 files

The existing report noted 17 files with clippy allows. The actual scope is much larger: **70 files** contain `#[allow(dead_code)]` annotations, many without justification comments. This over-suppression hides genuinely unused code.

Notable clusters:
- `crates/fraiseql-server/src/encryption/mod.rs`
- `crates/fraiseql-server/src/auth/oauth.rs`
- `crates/fraiseql-core/src/federation/saga_executor.rs`
- `crates/fraiseql-cli/tests/federation_cross_subgraph_validation.rs` (4+ instances)

**Total `#![allow(...)]` in lib.rs files:** 123+ attributes across all crates.

### 9.2 Commented-Out Code in Production

- **`crates/fraiseql-core/src/runtime/executor.rs:297-303`** — 7-line block of field-level access control logic, commented out with `// TODO: Re-enable field-level access control once validate_field_access is implemented`
- **`crates/fraiseql-core/src/runtime/executor.rs:1309-1370`** — 60+ lines of commented-out test functions
- Scattered instances in `fraiseql-wire/tests/metrics_integration.rs:488`, `fraiseql-observers/src/tracing/tests.rs:258`, `fraiseql-server/tests/http_server_e2e_test.rs:418-421`

### 9.3 `expect()`/`unwrap()` in Production Paths — 1,029+ instances

Cryptographic and security-critical paths use `expect()` which panics the server on failure:
- `crates/fraiseql-server/src/auth/state_encryption.rs` — 30+ `expect()` calls on encrypt/decrypt operations
- `crates/fraiseql-server/src/auth/jwt.rs` — `expect()` on validator creation
- `crates/fraiseql-server/src/auth/postgres_audit_logger.rs` — `expect()` on HMAC creation

These should return `Result<T, Error>` for graceful degradation instead of crashing the server.

### 9.4 Acknowledged Race Condition

- **File:** `crates/fraiseql-server/src/auth/rate_limiting.rs:167`
- Comment acknowledges a TOCTOU race between checking `config.enabled` and acquiring a lock, but the race is not mitigated.

---

## 10. Python Code Quality (Beyond Existing Report)

### 10.1 Deprecated Type Annotations — 557 instances

The project targets Python 3.13 but uses old-style type hints throughout:
- **544 instances** of `Optional[X]` (should be `X | None`)
- **13 instances** of `Union[X, Y]` (should be `X | Y`)
- Imports of `List`, `Dict`, `Set` from `typing` (should use builtins)

Representative files: `src/fraiseql/debug/debug.py:12,30,276,277`, `src/fraiseql/auth/rust_provider.py:8`, `src/fraiseql/mutations/types.py:4`

### 10.2 Broad Exception Handlers — 38 instances

`except Exception:` used without specificity, masking real errors:
- `src/fraiseql/auth/token_revocation.py:429`
- `src/fraiseql/security/security_headers.py:538`
- `src/fraiseql/auth/native/tokens.py:249` (returns `None`, swallowing the error)
- `src/fraiseql/decorators.py:535,610`
- `src/fraiseql/optimization/n_plus_one_detector.py:255,290,318`

### 10.3 F-String Logging — Performance Anti-pattern

- **File:** `src/fraiseql/storage/backends/factory.py:32,66,91`
- Uses `logger.debug(f"Creating APQ backend: type={backend_type}")` — f-strings are evaluated eagerly even when the log level is above DEBUG.
- Should use `logger.debug("Creating APQ backend: type=%s", backend_type)`.

### 10.4 Unused Import

- **File:** `src/fraiseql/debug/debug.py:9` — `import json` is imported but never used.

### 10.5 Hardcoded Fallback Secret

- **File:** `src/fraiseql/auth/native/router.py:176`
- `secret_key = os.environ.get("JWT_SECRET_KEY", "test-secret-key-change-in-production")`
- A missing env var silently falls back to a known secret. Should raise an error in production.

---

## 11. Test Suite Structural Issues

### 11.1 Scale

| Layer | Test Files | Test Functions |
|-------|-----------|----------------|
| Python (`tests/`) | 690 | ~1,500+ |
| Rust (`crates/*/tests/`) | 193 | 2,187 |
| Rust (inline `#[cfg(test)]`) | 25+ modules | ~700+ |
| **Total** | **~908** | **~4,400+** |

### 11.2 Python Tests Not Executed in Main CI

The main CI workflow (`ci.yml`) runs **only Rust tests**. The 690 Python test files are only exercised by:
- `chaos-engineering-tests.yml` (nightly/manual, runs `pytest tests/chaos` only)
- `fraisier-ci.yml` (covers only the `fraisier/` subdirectory)

The bulk of `tests/unit/`, `tests/integration/`, `tests/regression/`, `tests/system/` are **never run in CI**.

### 11.3 35+ Python Test Files Import Deleted Modules

All tests importing from `fraiseql.fastapi`, `fraiseql.execution`, `fraiseql.cqrs`, `fraiseql.gql.graphql_entrypoint`, or `fraiseql.subscriptions.websocket` will fail at import time. This affects:
- `tests/integration/auth/test_schema_introspection_security.py`
- `tests/integration/auth/test_auth_enforcement.py`
- `tests/unit/execution/test_mode_selector.py`
- `tests/integration/graphql/subscriptions/test_websocket_subscriptions.py`
- 31+ additional test files

### 11.4 Rust Test Files in `src/` Instead of `tests/`

Four significant test modules live inside `src/` as `#[cfg(test)]` modules rather than in `tests/`:
- `crates/fraiseql-server/src/encryption/compliance_tests.rs` (1,166 lines, 32 tests)
- `crates/fraiseql-server/src/encryption/query_builder_integration_tests.rs` (1,142 lines, 80+ tests)
- `crates/fraiseql-server/src/encryption/rotation_api_tests.rs` (200+ lines)
- `crates/fraiseql-server/src/secrets/schema_tests.rs` (200+ lines)

These are integration-level tests that would be better organized in `crates/fraiseql-server/tests/`.

---

## 12. CI and Configuration Health

### 12.1 Type Checker Mismatch

- `pyproject.toml` specifies `ty >= 0.0.1a28` as a dev dependency
- `fraisier-ci.yml` installs and runs `mypy` (line 113)
- No `[tool.mypy]` or `[tool.ty]` section in pyproject.toml
- **Inconsistency:** Two different type checkers across different contexts, neither configured.

### 12.2 Benchmark Job Unreachable on Release Branch

- **File:** `.github/workflows/ci.yml:461`
- Condition: `if: github.ref == 'refs/heads/v2-development'`
- Benchmarks will never run on the `release/v2.0.0-alpha.3` branch.

### 12.3 Security Audit Suppressions (5 advisories)

All documented with review date `2026-02-09`:
- RUSTSEC-2023-0071 (rsa timing sidechannel)
- RUSTSEC-2024-0384 (instant — unmaintained)
- RUSTSEC-2024-0436 (paste — unmaintained)
- RUSTSEC-2025-0134 (rustls-pemfile — unmaintained)
- RUSTSEC-2026-0002 (lru — stacked borrows)

### 12.4 Broken Examples — 10+ files

Examples referencing deleted FastAPI modules will fail:
- `examples/turbo_router_with_complexity.py` — imports `create_fraiseql_app`, `TurboQuery`, `EnhancedTurboRegistry`
- `examples/documented_api.py`
- `examples/admin-panel/main.py`
- `examples/saas-starter/main.py`
- `examples/token_revocation_example.py`
- And 5+ more

### 12.5 Orphaned Type Stub

- **File:** `src/fraiseql/fastapi.pyi` (3,497 bytes)
- Defines signatures for `FraiseQLConfig`, `create_fraiseql_app()`, `TurboRouter`
- No corresponding implementation exists (entire `fastapi/` directory deleted)

---

## 13. Revised Metrics Dashboard

| Metric | Original Report | Extended Finding | Target |
|--------|----------------|-----------------|--------|
| Rust compiler errors | _not covered_ | **7 errors (blocks build)** | 0 |
| Python import failures | _not covered_ | **Package unimportable** | 0 |
| Version mismatches | _not covered_ | **3 systems diverged** | All aligned |
| Rust clippy allows | 17 files | **70 files** with `#[allow(dead_code)]`, **123+ attrs** in lib.rs | 0 files |
| Clippy warnings | 6+ | 6+ (confirmed) | 0 |
| `expect()`/`unwrap()` in prod | _not covered_ | **1,029+ instances** | Audit & reduce |
| Commented-out code (Rust) | _not covered_ | **~80 lines** across 5+ files | 0 lines |
| Python TODO/FIXME | 19 files | 33 items (cataloged in `TODO_AUDIT.md`) | 0 |
| Deprecated type annotations | _not covered_ | **557 instances** (`Optional`, `Union`) | 0 |
| Broad `except Exception:` | _not covered_ | **38 instances** | 0 |
| Broken test files (Python) | _not covered_ | **35+ files** import deleted modules | 0 |
| Broken example files | _not covered_ | **10+ files** reference deleted modules | 0 |
| Python tests in CI | _not covered_ | **690 files, 0 in main CI** | All in CI |
| Test utility duplication | 3 locations | 3 locations (confirmed) | 1 crate |
| Doc coverage (Rust) | ~75% | ~75% (confirmed) | 90% |
| Type coverage (Python) | ~57% | ~57% (confirmed) | 90% |

---

## 14. Revised Action Plan

### Immediate (blocks this release)

- [ ] Fix Rust compiler errors: `executor_wrapper.rs` (method name + type annotations), `graphql.rs` (add `introspection_enabled` to 4 test configs)
- [ ] Fix Python import chain: remove or stub `CQRSExecutor` import in `__init__.py`, fix `execution/__init__.py`, fix `cli/commands/__init__.py`
- [ ] Fix async property in `auth/auth0.py` (convert to regular async method)
- [ ] Align version numbers: Python pyproject.toml, Dockerfile label, fraiseql-wire crate
- [ ] Remove orphaned `src/fraiseql/fastapi.pyi` type stub

### Week 1: Stabilize

- [ ] Audit and fix or delete 35+ broken Python test files
- [ ] Audit and fix or delete 10+ broken example files
- [ ] Remove dead code: `executor.rs` unused imports, dead methods, commented blocks
- [ ] Remove unused `import json` in `debug/debug.py`
- [ ] Add Python test jobs to main CI workflow

### Month 1: Quality

- [ ] Centralize 123+ clippy allows into workspace `Cargo.toml`
- [ ] Audit 70 files with `#[allow(dead_code)]` — remove suppression or delete dead code
- [ ] Audit `expect()`/`unwrap()` in auth/encryption paths — convert to `Result`
- [ ] Modernize Python type annotations (`Optional` → `X | None`)
- [ ] Fix 38 broad exception handlers
- [ ] Choose and configure one type checker (`ty` or `mypy`) consistently

### Quarter 1: Harden

- [ ] Create `fraiseql-test-utils` crate
- [ ] Move integration-level test modules from `src/` to `tests/`
- [ ] Consolidate observability modules in fraiseql-server
- [ ] Fix rate limiting race condition
- [ ] Replace hardcoded JWT fallback secret with mandatory env var
- [ ] Update CI benchmark job to run on release branches
