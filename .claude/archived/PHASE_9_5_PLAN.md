# Phase 9.5: Explicit DDL Generation for Table-Backed Views

**Version:** 1.0
**Status:** Planning
**Author:** Architecture Team
**Date:** January 24, 2026

---

## Executive Summary

Phase 9.5 implements helper libraries and tooling to **explicitly** generate DDL for table-backed views (tv_* and ta_*). Following FraiseQL's philosophy of explicit over implicit, DDL generation is a **tool developers call**, not an automatic optimization the compiler performs.

**Philosophy:** Developers decide to use table-backed views (via Phase 9.4 guides), then use Phase 9.5 tools to generate DDL for their explicit choice.

**Scope:** Documentation + Python helper + TypeScript helper + CLI command + tests

**Effort:** ~5 hours

**Risk:** Very low (no compiler modifications)

---

## Context & Problem Statement

### Current State

✅ Phase 9.4 complete: Comprehensive view selection documentation
✅ Developers understand WHEN to use tv_* and ta_*
✅ Manual SQL examples exist (tv_user_profile.sql, tv_order_summary.sql)

### Problem

❌ Once developer decides to use tv_user_profile, how do they generate DDL?
❌ Manual SQL editing is error-prone and inconsistent
❌ No Python/TypeScript helpers exist
❌ No CLI command for non-programmers

### Solution

Provide **explicit, intentional** tools to generate DDL for chosen views:
- Developer reads Phase 9.4 guides → Decides to use tv_user_profile
- Developer calls `generate_tv_ddl()` → Gets ready-to-run SQL
- Developer reviews SQL → Approves before deploying

This is an **implementation tool**, not a decision-making tool.

---

## Objectives

1. **Reduce manual effort** for DDL generation
2. **Improve consistency** across all generated table-backed views
3. **Make DDL generation accessible** to both programmers and non-programmers
4. **Maintain explicit philosophy** (developer in control at all times)
5. **Set foundation** for future compiler integration (Option 3)

---

## Architecture Overview

### Four Implementation Paths

Developers can generate DDL via:

1. **Python Helper Library**
   ```python
   from fraiseql_tools.views import generate_tv_ddl
   ddl = generate_tv_ddl(schema, entity="User", view="tv_user_profile")
   ```

2. **TypeScript Helper Library**
   ```typescript
   import { generateTvDdl } from "@fraiseql/tools"
   const ddl = generateTvDdl(schema, "User", "tv_user_profile")
   ```

3. **CLI Command**
   ```bash
   fraiseql generate-views --entity User --view tv_user_profile --output views.sql
   ```

4. **Web UI** (optional, future)
   - Provide interactive form
   - Show preview of generated DDL

### Core Logic

All paths use same underlying generation logic:

```
Schema Analysis
  ↓ (read schema.json)
Entity Extraction
  ↓ (find entity definition)
Relationship Analysis
  ↓ (find related entities)
Composition View Generation
  ↓ (generate helper views for nesting)
Table-Backed View Generation
  ↓ (generate tv_* or ta_* table)
Refresh Function Generation
  ↓ (generate refresh logic)
Output Assembly
  ↓ (combine into single SQL file)
Ready-to-Run DDL
```

### Supported View Types

**JSON Plane (GraphQL):**
- `tv_*` - Table-backed JSON views with JSONB composition

**Arrow Plane (Analytics):**
- `ta_*` - Table-backed Arrow views with columnar storage

**Refresh Strategies:**
- `trigger-based` - Real-time via database triggers
- `scheduled` - Batch via pg_cron

---

## Implementation Plan

### Task 1: Create DDL Generation Guide (~30 min)

**File:** `docs/guides/ddl-generation-guide.md`

**Purpose:** Explain when and how to use DDL generation helpers

**Sections:**

1. **Overview**
   - What this guide does (generates DDL for explicit choices)
   - What it does NOT do (make optimization decisions)
   - When to use (after deciding to use tv_* or ta_*)

2. **Quick Start**
   ```python
   from fraiseql_tools.views import generate_tv_ddl

   schema = load_schema("schema.json")
   ddl = generate_tv_ddl(
       schema=schema,
       entity="User",
       view="tv_user_profile",
       refresh_strategy="trigger-based"
   )

   with open("views.sql", "w") as f:
       f.write(ddl)
   ```

3. **Detailed Usage**
   - Python examples
   - TypeScript examples
   - CLI examples

4. **Parameters Explained**
   - `schema`: The loaded schema.json
   - `entity`: Entity name (e.g., "User", "Order")
   - `view`: View name (e.g., "tv_user_profile")
   - `refresh_strategy`: "trigger-based" or "scheduled"
   - `include_composition_views`: Include helper views (default: True)
   - `include_monitoring_functions`: Include staleness/verification functions (default: True)

5. **Output Structure**
   ```sql
   -- Generated DDL contains:
   -- 1. Composition helper views (v_*_composed)
   -- 2. Physical table (tv_* or ta_*)
   -- 3. Indexes
   -- 4. Refresh function
   -- 5. Refresh trigger(s)
   -- 6. Monitoring functions
   ```

6. **Next Steps**
   - Review generated SQL
   - Test in staging environment
   - Follow view-selection-migration-checklist.md
   - Deploy to production

7. **Cross-References**
   - Link to Phase 9.4 guides
   - Link to migration checklist
   - Link to performance testing guide

**Acceptance Criteria:**
- Document is clear and concise
- All code examples are correct
- Links to Phase 9.4 and migration guides
- Explains relationship to explicit philosophy

---

### Task 2: Implement Python Helper Library (~1 hour)

**Location:** `tools/fraiseql_tools/views.py` (NEW)

**Requires:** `tools/fraiseql_tools/__init__.py` already exists

**Functions to Implement:**

#### 1. `load_schema(path: str) -> dict`
```python
def load_schema(path: str) -> dict:
    """Load schema.json and return as dict."""
    with open(path, 'r') as f:
        return json.load(f)
```

Purpose: Load schema.json for analysis

#### 2. `generate_tv_ddl(...) -> str`
```python
def generate_tv_ddl(
    schema: dict,
    entity: str,
    view: str,
    refresh_strategy: str = "trigger-based",
    include_composition_views: bool = True,
    include_monitoring_functions: bool = True
) -> str:
    """Generate DDL for table-backed JSON view (tv_*)."""
    # Returns complete, ready-to-run SQL
```

Purpose: Generate tv_* DDL from schema

**Implementation Logic:**
1. Extract entity from schema
2. Identify relationships (posts, comments, etc.)
3. Generate composition views for nesting
4. Generate tv_* table schema
5. Generate refresh function (trigger or scheduled)
6. Generate monitoring functions (optional)
7. Return complete SQL string

#### 3. `generate_ta_ddl(...) -> str`
```python
def generate_ta_ddl(
    schema: dict,
    entity: str,
    view: str,
    refresh_strategy: str = "scheduled",
    include_monitoring_functions: bool = True
) -> str:
    """Generate DDL for table-backed Arrow view (ta_*)."""
    # Returns complete, ready-to-run SQL
```

Purpose: Generate ta_* DDL from schema

**Implementation Logic:**
1. Extract entity from schema
2. Identify columns to extract
3. Determine index strategy (BRIN for time-series)
4. Generate ta_* table schema
5. Generate refresh function
6. Generate monitoring functions (optional)
7. Return complete SQL string

#### 4. `generate_composition_views(...) -> str`
```python
def generate_composition_views(
    schema: dict,
    entity: str,
    relationships: list[str]
) -> str:
    """Generate helper composition views for nested relationships."""
    # Returns SQL for v_*_composed views
```

Purpose: Generate intermediate views for JSONB composition

#### 5. `suggest_refresh_strategy(...) -> str`
```python
def suggest_refresh_strategy(
    write_volume: int,  # writes per minute
    latency_requirement_ms: int,
    read_volume: int  # requests per second
) -> str:
    """Suggest refresh strategy based on workload characteristics."""
    # Returns "trigger-based" or "scheduled"
```

Purpose: Helper to recommend strategy (informational, not used by generators)

#### 6. `validate_generated_ddl(sql: str) -> list[str]`
```python
def validate_generated_ddl(sql: str) -> list[str]:
    """Validate generated DDL syntax and structure."""
    # Returns list of validation errors (empty if valid)
```

Purpose: Catch issues before deploying

**Implementation Details:**

- Read templates from `tools/fraiseql_tools/templates/`
- Use Jinja2 for SQL template rendering
- Support both PostgreSQL and future databases
- Include comprehensive comments in generated SQL
- Generate idempotent DDL (CREATE OR REPLACE where possible)

**Template Files to Create:**

1. `tools/fraiseql_tools/templates/tv_base.sql` - Base table-backed view
2. `tools/fraiseql_tools/templates/ta_base.sql` - Arrow table-backed view
3. `tools/fraiseql_tools/templates/composition_view.sql` - Helper views
4. `tools/fraiseql_tools/templates/refresh_trigger.sql` - Trigger function
5. `tools/fraiseql_tools/templates/refresh_scheduled.sql` - Batch refresh
6. `tools/fraiseql_tools/templates/monitoring.sql` - Monitoring functions

**Acceptance Criteria:**
- ✅ All 6 functions implemented
- ✅ Templates directory structured
- ✅ Python code passes clippy/linting
- ✅ Error handling for invalid inputs
- ✅ Comprehensive docstrings
- ✅ No hardcoded values (all configurable)

---

### Task 3: Implement TypeScript Helper Library (~1 hour)

**Location:** `packages/@fraiseql/tools/src/views.ts` (NEW)

**Purpose:** Same capability as Python helper for TypeScript users

**Functions to Implement:**

1. `loadSchema(path: string): object`
2. `generateTvDdl(options: GenerateTvOptions): string`
3. `generateTaDdl(options: GenerateTaOptions): string`
4. `generateCompositionViews(options: CompositionOptions): string`
5. `suggestRefreshStrategy(options: StrategyOptions): string`
6. `validateGeneratedDdl(sql: string): string[]`

**Type Definitions:**
```typescript
interface GenerateTvOptions {
    schema: SchemaObject;
    entity: string;
    view: string;
    refreshStrategy?: "trigger-based" | "scheduled";
    includeCompositionViews?: boolean;
    includeMonitoringFunctions?: boolean;
}

interface GenerateTaOptions {
    schema: SchemaObject;
    entity: string;
    view: string;
    refreshStrategy?: "scheduled" | "trigger-based";
    includeMonitoringFunctions?: boolean;
}
```

**Implementation:**

- Reuse SQL templates from Python package
- Load templates at runtime or build-time
- Same logic flow as Python
- Support CommonJS and ESM

**Usage Example:**
```typescript
import { generateTvDdl, loadSchema } from "@fraiseql/tools/views";

const schema = loadSchema("schema.json");
const ddl = generateTvDdl({
    schema,
    entity: "User",
    view: "tv_user_profile",
    refreshStrategy: "trigger-based"
});

console.log(ddl);
```

**Acceptance Criteria:**
- ✅ All 6 functions implemented
- ✅ TypeScript strict mode passes
- ✅ Type definitions comprehensive
- ✅ Error handling consistent with Python
- ✅ Comprehensive JSDoc comments
- ✅ No external dependencies beyond JSON

---

### Task 4: Implement CLI Command (~1 hour)

**Location:** `crates/fraiseql-cli/src/commands/generate_views.rs` (NEW)

**Purpose:** Make DDL generation accessible to non-programmers

**Command Structure:**
```bash
fraiseql generate-views [OPTIONS] --schema <SCHEMA> --entity <ENTITY> --view <VIEW>
```

**Options:**
- `--schema <PATH>` - Path to schema.json (required)
- `--entity <NAME>` - Entity name (required)
- `--view <NAME>` - View name (required)
- `--refresh-strategy <STRATEGY>` - "trigger-based" or "scheduled" (default: "trigger-based")
- `--output <PATH>` - Output file (default: "{view}.sql")
- `--include-composition-views` - Include helper views (default: true)
- `--include-monitoring` - Include monitoring functions (default: true)
- `--validate` - Only validate, don't write file
- `--verbose` - Show generation steps

**Implementation:**

1. Parse CLI arguments using Clap
2. Load schema.json
3. Validate inputs
4. Call Python helper library via subprocess OR implement in Rust
5. Write output file
6. Show success message with file location

**Option:** Implement in Rust directly (no subprocess) using same templates

**Usage Examples:**
```bash
# Generate trigger-based tv_user_profile
fraiseql generate-views \
  --schema schema.json \
  --entity User \
  --view tv_user_profile \
  --output views.sql

# Generate scheduled ta_orders with monitoring
fraiseql generate-views \
  --schema schema.json \
  --entity Order \
  --view ta_orders \
  --refresh-strategy scheduled \
  --include-monitoring

# Validate only (don't write file)
fraiseql generate-views \
  --schema schema.json \
  --entity User \
  --view tv_user_profile \
  --validate
```

**Integration:**

1. Add to `Commands` enum in `main.rs`
2. Add subcommand handler to match statement
3. Follow existing CLI patterns (compile, validate, introspect)

**Acceptance Criteria:**
- ✅ Command parses all options correctly
- ✅ Loads and validates schema.json
- ✅ Generates DDL using helper logic
- ✅ Writes output file (or validates only)
- ✅ Shows clear success/error messages
- ✅ Help text complete and accurate
- ✅ Follows existing CLI patterns

---

### Task 5: Create Tests & Examples (~1.5 hours)

**Test Files:**

#### `tools/fraiseql_tools/tests/test_views.py`
```python
def test_generate_tv_ddl_basic():
    """Generate tv_* for simple entity."""
    schema = load_test_schema("user")
    ddl = generate_tv_ddl(schema, "User", "tv_user_profile")
    assert "CREATE TABLE tv_user_profile" in ddl
    assert "JSONB" in ddl
    assert "TRIGGER" in ddl

def test_generate_tv_ddl_with_relationships():
    """Generate tv_* for entity with relationships."""
    schema = load_test_schema("user_with_posts")
    ddl = generate_tv_ddl(schema, "User", "tv_user_profile")
    assert "v_posts_by_user" in ddl  # Composition view
    assert "jsonb_agg" in ddl
    assert "ORDER BY" in ddl

def test_generate_ta_ddl():
    """Generate ta_* for Arrow view."""
    schema = load_test_schema("orders")
    ddl = generate_ta_ddl(schema, "Order", "ta_orders")
    assert "CREATE TABLE ta_orders" in ddl
    assert "BRIN" in ddl  # Time-series index

def test_validate_generated_ddl():
    """Validate generated SQL."""
    schema = load_test_schema("user")
    ddl = generate_tv_ddl(schema, "User", "tv_user_profile")
    errors = validate_generated_ddl(ddl)
    assert len(errors) == 0

def test_sql_syntax_valid():
    """Ensure generated SQL is syntactically valid."""
    # Parse with sqlparse or similar
    schema = load_test_schema("user")
    ddl = generate_tv_ddl(schema, "User", "tv_user_profile")
    parsed = sqlparse.parse(ddl)
    assert len(parsed) > 0  # Successfully parsed
```

#### `packages/@fraiseql/tools/tests/views.test.ts`
```typescript
describe("generateTvDdl", () => {
    it("should generate tv_* DDL", () => {
        const schema = loadTestSchema("user");
        const ddl = generateTvDdl({
            schema,
            entity: "User",
            view: "tv_user_profile"
        });
        expect(ddl).toContain("CREATE TABLE tv_user_profile");
        expect(ddl).toContain("JSONB");
    });

    it("should include composition views", () => {
        const schema = loadTestSchema("user_with_posts");
        const ddl = generateTvDdl({
            schema,
            entity: "User",
            view: "tv_user_profile",
            includeCompositionViews: true
        });
        expect(ddl).toContain("v_posts_by_user");
    });
});
```

#### `crates/fraiseql-cli/tests/generate_views.rs`
```rust
#[tokio::test]
async fn test_cli_generate_views() {
    // Test CLI command works end-to-end
    let output = run_command(&[
        "generate-views",
        "--schema", "test_schemas/user.json",
        "--entity", "User",
        "--view", "tv_user_profile"
    ]).await;

    assert!(output.contains("CREATE TABLE tv_user_profile"));
}

#[tokio::test]
async fn test_cli_validate_only() {
    // Test --validate flag
    let output = run_command(&[
        "generate-views",
        "--schema", "test_schemas/user.json",
        "--entity", "User",
        "--view", "tv_user_profile",
        "--validate"
    ]).await;

    assert!(output.contains("Valid DDL"));
    assert!(!std::path::Path::new("tv_user_profile.sql").exists());
}
```

**Example Files:**

#### `examples/ddl-generation/python-example.py`
```python
#!/usr/bin/env python3
"""Example: Generate DDL for tv_user_profile."""

from fraiseql_tools.views import generate_tv_ddl, load_schema

# Load your schema
schema = load_schema("schema.json")

# Generate DDL for User → tv_user_profile
ddl = generate_tv_ddl(
    schema=schema,
    entity="User",
    view="tv_user_profile",
    refresh_strategy="trigger-based"
)

# Write to file
with open("views.sql", "w") as f:
    f.write(ddl)

print(f"Generated {len(ddl)} bytes of DDL")
print("File: views.sql")
print("\nTo deploy:")
print("  psql -h localhost -U postgres mydb < views.sql")
```

#### `examples/ddl-generation/typescript-example.ts`
```typescript
import { generateTvDdl, loadSchema } from "@fraiseql/tools/views";
import { writeFileSync } from "fs";

// Load your schema
const schema = loadSchema("schema.json");

// Generate DDL for User → tv_user_profile
const ddl = generateTvDdl({
    schema,
    entity: "User",
    view: "tv_user_profile",
    refreshStrategy: "trigger-based"
});

// Write to file
writeFileSync("views.sql", ddl);

console.log(`Generated ${ddl.length} bytes of DDL`);
console.log("File: views.sql");
console.log("\nTo deploy:");
console.log("  psql -h localhost -U postgres mydb < views.sql");
```

#### `examples/ddl-generation/cli-example.sh`
```bash
#!/bin/bash
# Example: Generate DDL using CLI command

# Generate trigger-based tv_user_profile
fraiseql generate-views \
  --schema schema.json \
  --entity User \
  --view tv_user_profile \
  --refresh-strategy trigger-based \
  --output tv_user_profile.sql

# Generate scheduled ta_orders
fraiseql generate-views \
  --schema schema.json \
  --entity Order \
  --view ta_orders \
  --refresh-strategy scheduled \
  --output ta_orders.sql

# Combine both into single file
cat tv_user_profile.sql ta_orders.sql > all_views.sql

# Deploy to staging
psql -h staging-db -U postgres mydb < all_views.sql

# Deploy to production
psql -h prod-db -U postgres mydb < all_views.sql
```

**Acceptance Criteria:**
- ✅ 5+ unit tests for Python helper
- ✅ 5+ unit tests for TypeScript helper
- ✅ 3+ integration tests for CLI
- ✅ All tests pass
- ✅ 3 example scripts (Python, TS, bash)
- ✅ Examples are runnable (with test schemas)

---

## Files to Create

### Documentation
- [ ] `docs/guides/ddl-generation-guide.md` - Usage guide

### Python Package
- [ ] `tools/fraiseql_tools/views.py` - Main implementation
- [ ] `tools/fraiseql_tools/templates/tv_base.sql` - TV template
- [ ] `tools/fraiseql_tools/templates/ta_base.sql` - TA template
- [ ] `tools/fraiseql_tools/templates/composition_view.sql` - Composition template
- [ ] `tools/fraiseql_tools/templates/refresh_trigger.sql` - Trigger template
- [ ] `tools/fraiseql_tools/templates/refresh_scheduled.sql` - Scheduled template
- [ ] `tools/fraiseql_tools/templates/monitoring.sql` - Monitoring template
- [ ] `tools/fraiseql_tools/tests/test_views.py` - Tests

### TypeScript Package
- [ ] `packages/@fraiseql/tools/src/views.ts` - Implementation
- [ ] `packages/@fraiseql/tools/tests/views.test.ts` - Tests

### CLI
- [ ] `crates/fraiseql-cli/src/commands/generate_views.rs` - Command implementation
- [ ] `crates/fraiseql-cli/tests/generate_views.rs` - Tests

### Examples
- [ ] `examples/ddl-generation/python-example.py`
- [ ] `examples/ddl-generation/typescript-example.ts`
- [ ] `examples/ddl-generation/cli-example.sh`

### Test Schemas
- [ ] `examples/ddl-generation/test_schemas/user.json` - Simple entity
- [ ] `examples/ddl-generation/test_schemas/user_with_posts.json` - With relationships
- [ ] `examples/ddl-generation/test_schemas/orders.json` - For ta_* testing

---

## Files to Update

- [ ] `docs/README.md` - Add link to DDL generation guide
- [ ] `tools/fraiseql_tools/__init__.py` - Export functions
- [ ] `packages/@fraiseql/tools/package.json` - Update exports
- [ ] `crates/fraiseql-cli/src/main.rs` - Add generate-views command
- [ ] `crates/fraiseql-cli/src/commands/mod.rs` - Add module

---

## Verification Strategy

### Pre-Implementation
- [ ] Review this plan with team
- [ ] Confirm template structure
- [ ] Identify any schema.json parsing edge cases

### During Implementation
- [ ] Test each function individually
- [ ] Run unit tests for each helper
- [ ] Verify generated SQL syntax

### Post-Implementation
- [ ] Run full test suite
- [ ] Test all three usage paths (Python, TS, CLI)
- [ ] Run examples on test schemas
- [ ] Verify CLI integration
- [ ] Check documentation completeness

### Validation Checklist

**Python Helper:**
- [ ] All 6 functions implemented
- [ ] No clippy warnings
- [ ] 5+ unit tests pass
- [ ] Docstrings complete
- [ ] Error handling tested

**TypeScript Helper:**
- [ ] All 6 functions implemented
- [ ] No TypeScript errors
- [ ] 5+ unit tests pass
- [ ] JSDoc comments complete
- [ ] Error handling tested

**CLI Command:**
- [ ] Accepts all options
- [ ] Generates valid DDL
- [ ] Writes output file correctly
- [ ] --validate flag works
- [ ] Help text accurate

**Documentation:**
- [ ] Clear and concise
- [ ] All code examples work
- [ ] Cross-references accurate
- [ ] Explains explicit philosophy

**Examples:**
- [ ] All 3 examples run without errors
- [ ] Generated SQL is valid
- [ ] Examples are well-commented

---

## Acceptance Criteria

✅ **Documentation Completeness**
- [ ] DDL generation guide exists and is clear
- [ ] Explains when to use (after Phase 9.4 decision)
- [ ] Explains what it does NOT do (no automatic optimization)
- [ ] Shows all three usage paths (Python, TS, CLI)

✅ **Implementation Completeness**
- [ ] Python helper library fully implemented
- [ ] TypeScript helper library fully implemented
- [ ] CLI command integrated and tested
- [ ] All code passes linting and type checking

✅ **Test Coverage**
- [ ] 5+ unit tests for Python helper
- [ ] 5+ unit tests for TypeScript helper
- [ ] 3+ integration tests for CLI
- [ ] All tests pass
- [ ] Generated DDL validates

✅ **Code Quality**
- [ ] No warnings from linters/checkers
- [ ] Comprehensive docstrings/JSDoc
- [ ] Error handling for invalid inputs
- [ ] Clear, readable code

✅ **Examples & Usability**
- [ ] Python example works end-to-end
- [ ] TypeScript example works end-to-end
- [ ] CLI example works end-to-end
- [ ] Bash example script provided

✅ **Explicit Philosophy**
- [ ] Developer must decide first (Phase 9.4)
- [ ] Developer explicitly calls generate functions
- [ ] Developer reviews generated SQL
- [ ] No automatic optimization or magic

✅ **Foundation for Future**
- [ ] Helpers can be used by Option 3 compiler (future)
- [ ] Templates are reusable
- [ ] Core logic is isolated from CLI/helpers

---

## DO NOT / Guardrails

❌ **DO NOT** add automatic view recommendations
- Keep it explicit: Developer decides, then generates

❌ **DO NOT** modify the compiler
- This is a standalone tool, not a compiler phase

❌ **DO NOT** support multiple databases yet
- Focus on PostgreSQL only
- Future phases can add MySQL, SQL Server support

❌ **DO NOT** change Phase 9.4 documentation
- Don't repeat decision criteria
- Reference existing guides instead

❌ **DO NOT** create validation that's too strict
- Allow customization for advanced users
- Warn about risks, don't prevent

❌ **DO NOT** hardcode SQL patterns
- Use templates for all SQL generation
- Make templates reusable and clear

❌ **DO NOT** skip error handling
- Validate schema structure
- Provide helpful error messages
- Don't crash on invalid input

---

## Timeline & Dependencies

**Dependencies:**
- Phase 9.4 complete (view selection guides) ✅
- Option A complete (supplementary guides) ✅
- Schema.json format understood ✅

**Estimated Timeline:**
- Task 1 (Documentation): 30 min
- Task 2 (Python Helper): 1 hour
- Task 3 (TypeScript Helper): 1 hour
- Task 4 (CLI Command): 1 hour
- Task 5 (Tests & Examples): 1.5 hours
- **Total: ~5 hours**

**Can be parallelized:**
- Tasks 2 & 3 can happen simultaneously (Python + TS)
- Task 5 (tests) can start after Task 2 starts

---

## Success Metrics

After Phase 9.5 completes:

1. **Usability:** Developer can generate full tv_* DDL in <5 minutes
2. **Consistency:** All generated views follow same patterns
3. **Accessibility:** Non-programmers can use CLI command
4. **Quality:** 100% of generated DDL is valid SQL
5. **Documentation:** New developers can follow ddl-generation-guide.md

---

## Future Work (Not This Phase)

- Option 3: Compiler integration for automatic suggestions
- Multi-database support (MySQL, SQL Server)
- Web UI for DDL generation
- Integration with schema versioning
- Performance tracking for generated views
- Migration utilities (v_→tv_, va_→ta_)

---

## References

- [Phase 9.4: View Selection Guide](./PHASE_9_4_PLAN.md)
- [View Selection Architecture Docs](./docs/architecture/database/view-selection-guide.md)
- [TV Table Pattern](./docs/architecture/database/tv-table-pattern.md)
- [TA Table Pattern](./docs/architecture/database/ta-table-pattern.md)
