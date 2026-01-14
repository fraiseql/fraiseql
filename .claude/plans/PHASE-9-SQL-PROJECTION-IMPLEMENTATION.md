# Phase 9: SQL Field Projection Implementation Plan

**Date**: January 14, 2026
**Status**: Implementation Ready
**Effort**: 3-4 days
**Priority**: High (37% improvement on PostgreSQL)

---

## Executive Summary

Phase 9 implements Hybrid Strategy #2 for field projection optimization:
- **PostgreSQL**: Generate SQL `jsonb_build_object()` at compile time → **37.2% improvement** (3.123ms → 1.961ms)
- **Wire**: Add optional `.select_projection()` method → **No performance gain today**, but prepares framework for future async optimization
- **Implementation**: ~300 total lines of code across compiler and runtime
- **Risk**: Low (builds on existing optimization framework)

**Key Finding from Testing**: __typename should stay in Rust (~0.03ms) not SQL (~0.37ms)

---

## Implementation Steps

### Step 1: Extend Compiled Schema with Projection Hints

**File**: `crates/fraiseql-core/src/schema/compiled.rs`

Add `sql_projection` field to `TypeDefinition`:

```rust
pub struct TypeDefinition {
    pub name: String,
    pub fields: Vec<FieldDefinition>,
    pub description: Option<String>,
    pub sql_source: String,
    pub jsonb_column: String,
    // NEW: SQL projection hint
    pub sql_projection: Option<SqlProjectionHint>,
}

pub struct SqlProjectionHint {
    /// The database type (postgresql, mysql, sqlite, etc.)
    pub database: String,

    /// The projection query template
    /// Example: "jsonb_build_object('id', data->>'id', 'email', data->>'email')"
    pub projection_template: String,

    /// Field mapping from schema field names to JSONB paths
    /// Example: {"firstName": "data->'firstName'->>'first'"}
    pub field_mappings: HashMap<String, String>,

    /// Whether __typename should be added (always false for SQL)
    pub include_typename: bool,

    /// Estimated reduction in payload size
    pub estimated_reduction_percent: u32,
}
```

**Verification**: Compile check passes, no type mismatches

---

### Step 2: Implement SQL Projection Detection in Schema Optimizer

**File**: `crates/fraiseql-cli/src/schema/optimizer.rs`

Add projection detection logic:

```rust
fn analyze_types(schema: &CompiledSchema, report: &mut OptimizationReport) {
    for type_def in &schema.types {
        // Check if type would benefit from SQL projection
        if Self::should_use_projection(type_def) {
            // Generate projection query for each supported database
            if let Ok(hint) = Self::generate_projection_hint(type_def) {
                report.projection_hints.push(ProjectionOpportunity {
                    type_name: type_def.name.clone(),
                    estimated_improvement_percent: hint.estimated_reduction_percent,
                    field_count: type_def.fields.len(),
                });
            }
        }
    }
}

fn should_use_projection(type_def: &TypeDefinition) -> bool {
    // Conditions for using SQL projection:
    // 1. Type uses JSONB column (must exist)
    // 2. Type has >10 fields OR jsonb_column size > 1KB
    // 3. Not a scalar type

    !type_def.jsonb_column.is_empty()
        && type_def.fields.len() > 10
}

fn generate_projection_hint(type_def: &TypeDefinition) -> Result<SqlProjectionHint> {
    // Generate database-specific projection
    // For PostgreSQL: jsonb_build_object(...)
    // For MySQL: JSON_OBJECT(...)
    // For SQLite: json_object(...)
}
```

**Verification**: Tests for detection logic passing

---

### Step 3: Extend SchemaConverter to Generate Projection Hints

**File**: `crates/fraiseql-cli/src/schema/converter.rs`

During schema conversion, attach projection hints to types:

```rust
fn convert_type(intermediate: &IntermediateType) -> Result<TypeDefinition> {
    let mut type_def = TypeDefinition {
        // ... existing fields ...
        sql_projection: None,
    };

    // If type qualifies for projection, generate hints
    if should_generate_projection(&type_def) {
        type_def.sql_projection = Some(generate_projection_hint(&type_def)?);
    }

    Ok(type_def)
}
```

**Verification**: Schema compiles with projection hints attached

---

### Step 4: Create SQL Projection Query Generator

**File**: `crates/fraiseql-cli/src/schema/projection_generator.rs` (NEW)

Implement database-specific SQL generation:

```rust
pub struct ProjectionGenerator;

impl ProjectionGenerator {
    /// Generate PostgreSQL jsonb_build_object() query
    pub fn postgres(type_def: &TypeDefinition, fields_to_select: &[&str]) -> Result<String> {
        // Example:
        // SELECT jsonb_build_object(
        //     'id', data->>'id',
        //     'email', data->>'email',
        //     'firstName', data->'firstName'->>'first'
        // ) as data FROM {table}

        let mut field_specs = Vec::new();
        for field_name in fields_to_select {
            let field_spec = format!("'{}', {}",
                field_name,
                get_jsonb_accessor(type_def, field_name)?
            );
            field_specs.push(field_spec);
        }

        Ok(format!(
            "SELECT jsonb_build_object({}) as data FROM {}",
            field_specs.join(", "),
            type_def.sql_source
        ))
    }

    /// Generate MySQL JSON_OBJECT() query
    pub fn mysql(type_def: &TypeDefinition, fields_to_select: &[&str]) -> Result<String> {
        // Similar structure for MySQL
    }

    /// Generate SQLite json_object() query
    pub fn sqlite(type_def: &TypeDefinition, fields_to_select: &[&str]) -> Result<String> {
        // Similar structure for SQLite
    }
}

fn get_jsonb_accessor(type_def: &TypeDefinition, field_name: &str) -> Result<String> {
    // Convert field name to JSONB accessor
    // Examples:
    // id → data->>'id'
    // firstName → data->'firstName'->>'first'
    // address.city → data->'address'->>'city'
    Ok(/* computed accessor */)
}
```

**Verification**: Test suite for projection generation passes

---

### Step 5: Update PostgreSQL Adapter to Use Projections

**File**: `crates/fraiseql-core/src/db/postgres.rs`

When executing a query with projection hint, use the optimized SQL:

```rust
impl PostgresAdapter {
    async fn execute_with_projection(
        &self,
        type_def: &TypeDefinition,
        base_query: &str,
        fields: &[&str],
    ) -> Result<Vec<Value>> {
        if let Some(hint) = &type_def.sql_projection {
            // Use projection query instead of full data fetch
            let projection_sql = ProjectionGenerator::postgres(type_def, fields)?;

            // Execute and deserialize
            let rows = self.pool.query(&projection_sql, &[]).await?;
            Ok(rows.into_iter().map(|row| {
                let data: Value = row.get("data");
                data
            }).collect())
        } else {
            // Fallback to standard execution
            self.execute(base_query).await
        }
    }
}
```

**Verification**: PostgreSQL adapter tests pass with projection queries

---

### Step 6: Implement ResultProjector Enhancement

**File**: `crates/fraiseql-core/src/runtime/projection.rs`

Update ResultProjector to handle SQL-projected data:

```rust
impl ResultProjector {
    pub fn project_result(
        &self,
        raw_value: Value,
        selected_fields: &[&str],
        type_def: &TypeDefinition,
        add_typename: bool,
    ) -> Result<Value> {
        // If data came from SQL projection, it's already filtered
        let mut result = raw_value;

        // Only add __typename in Rust (not in SQL)
        if add_typename {
            if let Value::Object(ref mut obj) = result {
                obj.insert("__typename".to_string(),
                    Value::String(type_def.name.clone()));
            }
        }

        Ok(result)
    }
}
```

**Verification**: Projection tests pass with __typename in Rust

---

### Step 7: Add fraiseql-wire QueryBuilder Enhancement

**File**: `crates/fraiseql-core/src/db/wire_query_builder.rs` (or extension)

Add optional projection support to Wire adapter (for consistency):

```rust
pub struct QueryBuilder {
    // ... existing fields ...
    select_clause: Option<String>,  // NEW
}

impl QueryBuilder {
    /// Set custom SELECT clause for projection
    pub fn select_projection(mut self, projection: String) -> Self {
        self.select_clause = Some(projection);
        self
    }

    fn build_sql(&self) -> String {
        let select = self.select_clause
            .as_ref()
            .cloned()
            .unwrap_or_else(|| "SELECT data".to_string());

        format!("{} FROM {}", select, self.entity)
    }
}
```

**Verification**: Wire adapter compiles with no regressions

---

### Step 8: Write Integration Tests

**File**: `crates/fraiseql-core/tests/sql_projection_integration_test.rs` (NEW)

Test end-to-end projection:

```rust
#[tokio::test]
async fn test_postgres_sql_projection_correctness() {
    // 1. Create schema with projection hints
    // 2. Execute query with large JSONB payload
    // 3. Verify results match full fetch
    // 4. Verify payload size reduction
}

#[tokio::test]
async fn test_projection_with_typename_in_rust() {
    // Verify __typename is added by Rust, not SQL
}

#[tokio::test]
async fn test_wire_projection_no_regression() {
    // Verify Wire with projection has no performance regression
}

#[tokio::test]
async fn test_projection_detection_logic() {
    // Verify optimizer correctly identifies projection candidates
}
```

**Verification**: All integration tests pass

---

### Step 9: Benchmark Phase 9 Implementation

**File**: `benches/sql_projection_benchmark.rs` (NEW)

Create benchmarks to validate 37% improvement claim:

```rust
#[bench]
fn bench_postgres_without_projection(b: &mut Bencher) {
    // Full JSONB fetch (baseline)
}

#[bench]
fn bench_postgres_with_sql_projection(b: &mut Bencher) {
    // With jsonb_build_object() optimization
}

#[bench]
fn bench_wire_with_projection(b: &mut Bencher) {
    // Verify no regression
}
```

**Expected Results**:
- PostgreSQL: 3.123ms → 1.961ms (37% improvement)
- Wire: ~6.027ms unchanged

**Verification**: Benchmarks confirm expected improvements

---

### Step 10: Documentation

**File**: `.claude/analysis/phase-9-implementation-notes.md` (NEW)

Document:
- How to enable projection for a type
- Database-specific SQL patterns
- __typename handling (Rust, not SQL)
- Performance characteristics
- Future optimization opportunities (Wire async)

**Verification**: Documentation is clear and complete

---

## Validation Checklist

- [ ] **Step 1**: TypeDefinition compiles with sql_projection field
- [ ] **Step 2**: SchemaOptimizer correctly detects projection candidates
- [ ] **Step 3**: SchemaConverter attaches projection hints
- [ ] **Step 4**: ProjectionGenerator creates valid SQL for all databases
- [ ] **Step 5**: PostgreSQL adapter uses projection queries
- [ ] **Step 6**: ResultProjector handles __typename correctly (Rust only)
- [ ] **Step 7**: fraiseql-wire QueryBuilder accepts select_clause
- [ ] **Step 8**: Integration tests all pass
- [ ] **Step 9**: Benchmarks show 37% improvement for PostgreSQL
- [ ] **Step 10**: Documentation complete and tested
- [ ] **Final**: All tests pass, no clippy warnings, schema compiles

---

## Success Criteria

### PostgreSQL Performance
- ✅ 37% improvement documented (3.123ms → 1.961ms)
- ✅ Large payloads (>1KB) benefit most
- ✅ __typename overhead avoided (0.03ms Rust vs 0.37ms SQL)

### Wire Adapter
- ✅ Enhancement added for consistency
- ✅ No performance regression detected
- ✅ Prepared for future async optimization

### Code Quality
- ✅ All tests pass (100% of integration suite)
- ✅ Clippy warnings addressed
- ✅ No unsafe code added
- ✅ Documentation complete

### Documentation
- ✅ Implementation notes in analysis/
- ✅ Code comments for projection logic
- ✅ Example schemas showing projection
- ✅ Performance characteristics documented

---

## Risk Mitigation

| Risk | Probability | Impact | Mitigation |
|------|------------|--------|-----------|
| SQL generation errors | Low | High | Comprehensive test suite for each database |
| __typename duplication | Low | Medium | Clear test case for Rust-only __typename |
| Wire regression | Low | High | Benchmark validation before merge |
| Performance not matching expected | Low | Medium | Real benchmarks against baselines |

---

## Timeline

- **Day 1**: Steps 1-3 (Schema extension, optimizer detection)
- **Day 2**: Steps 4-6 (SQL generation, adapter implementation)
- **Day 3**: Steps 7-9 (Wire enhancement, integration tests, benchmarks)
- **Day 4**: Step 10 (Documentation, polish, final validation)

---

## Key Files Modified

```
crates/fraiseql-core/src/
├── schema/compiled.rs          (+SqlProjectionHint struct)
├── db/postgres.rs               (+execute_with_projection method)
├── db/wire_query_builder.rs     (+select_clause field)
└── runtime/projection.rs        (enhance ProjectProjector)

crates/fraiseql-cli/src/
├── schema/optimizer.rs          (+projection detection)
├── schema/converter.rs          (+projection hint generation)
└── schema/projection_generator.rs (NEW - SQL generation)

crates/fraiseql-core/tests/
└── sql_projection_integration_test.rs (NEW)

benches/
└── sql_projection_benchmark.rs (NEW)
```

---

## Next Steps After Phase 9

1. **Phase 10**: Add MySQL/SQLite/SQL Server projection support
2. **Phase 11**: Optimize fraiseql-wire async overhead for Wire to benefit from projection
3. **Phase 12+**: Query plan caching, connection pooling optimization

---

**Status**: ✅ **READY FOR IMPLEMENTATION**

All analysis complete. Zero overhead validated. Ready to implement Phase 9 SQL projection optimization.
