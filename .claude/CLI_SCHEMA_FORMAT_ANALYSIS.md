# CLI Schema Format Analysis & Fix Plan

## Executive Summary

**Problem**: fraiseql-cli rejects schema.json from our language generators
**Root Cause**: Schema generators use `"return_list"` but CLI expects `"returns_list"`
**Impact**: Cannot run Phase 2 E2E compilation tests for all 10 languages
**Fix Scope**: Update all 10 language generators + velocitybench_schemas.py

---

## Detailed Analysis

### What CLI Expects (IntermediateSchema)

**File**: `crates/fraiseql-cli/src/schema/intermediate.rs`

#### IntermediateQuery Structure
```rust
pub struct IntermediateQuery {
    pub name: String,                           // REQUIRED
    pub return_type: String,                    // REQUIRED
    pub returns_list: bool,                     // ← KEY: "returns_list" not "return_list"
    pub nullable: bool,                         // Default: false
    pub arguments: Vec<IntermediateArgument>,   // Default: []
    pub description: Option<String>,            // Optional
    pub sql_source: Option<String>,             // Optional
    pub auto_params: Option<IntermediateAutoParams>, // Optional
}
```

#### IntermediateMutation Structure
```rust
pub struct IntermediateMutation {
    pub name: String,                           // REQUIRED
    pub return_type: String,                    // REQUIRED
    pub returns_list: bool,                     // ← KEY: "returns_list" not "return_list"
    pub nullable: bool,                         // Default: false
    pub arguments: Vec<IntermediateArgument>,   // Default: []
    pub description: Option<String>,            // Optional
    pub sql_source: Option<String>,             // Optional
    pub operation: Option<String>,              // Optional (CREATE, UPDATE, DELETE, CUSTOM)
}
```

#### IntermediateArgument Structure
```rust
pub struct IntermediateArgument {
    pub name: String,                           // REQUIRED
    pub arg_type: String,     // JSON: "type"  (uses #[serde(rename = "type")])
    pub nullable: bool,                         // REQUIRED
    pub default: Option<serde_json::Value>,     // Optional
}
```

#### IntermediateType Structure
```rust
pub struct IntermediateType {
    pub name: String,                           // REQUIRED
    pub fields: Vec<IntermediateField>,         // REQUIRED
    pub description: Option<String>,            // Optional
}
```

#### IntermediateField Structure
```rust
pub struct IntermediateField {
    pub name: String,                           // REQUIRED
    pub field_type: String,   // JSON: "type"  (uses #[serde(rename = "type")])
    pub nullable: bool,                         // REQUIRED
}
```

#### IntermediateSchema Structure
```rust
pub struct IntermediateSchema {
    pub version: String,                        // Default: "2.0.0"
    pub types: Vec<IntermediateType>,           // Default: []
    pub queries: Vec<IntermediateQuery>,        // Default: []
    pub mutations: Vec<IntermediateMutation>,   // Default: []
    pub fact_tables: Option<Vec<IntermediateFactTable>>, // Optional
    pub aggregate_queries: Option<Vec<IntermediateAggregateQuery>>, // Optional
}
```

---

## Current Schema Format Issues

### Issue #1: `return_list` vs `returns_list`

**Current (Wrong):**
```json
{
  "name": "users",
  "return_type": "User",
  "return_list": true,
  "sql_source": "v_users"
}
```

**Expected (Correct):**
```json
{
  "name": "users",
  "return_type": "User",
  "returns_list": true,
  "sql_source": "v_users"
}
```

### Files to Fix

#### 1. velocitybench_schemas.py
- Line 88: `"return_list": False` → `"returns_list": False`
- Line 109: `"return_list": True` → `"returns_list": True`
- Line 131: `"return_list": True` → `"returns_list": True`
- Line 151: `"return_list": True` → `"returns_list": True`
- Line 172: `"return_list": True` → `"returns_list": True`
- Line 192: `"return_list": True` → `"returns_list": True`
- Line 212: `"return_list": False` → `"returns_list": False`
- **Total**: 8 occurrences

#### 2. Python Schema Generator
Path: `fraiseql-python/fraiseql/schema.py` or similar
- All query/mutation definitions
- Pattern: `"return_list"` → `"returns_list"`

#### 3. TypeScript Schema Generator
Path: `fraiseql-typescript/src/schema.ts` or similar
- All query/mutation definitions
- Pattern: `"return_list"` → `"returns_list"`

#### 4. Go Schema Generator
Path: `fraisier/langgen/golang/generator.go` or similar
- All query/mutation definitions
- Pattern: `"return_list"` → `"returns_list"`

#### 5. Java Schema Generator
Path: `fraisier/langgen/java/generator.java` or similar
- All query/mutation definitions
- Pattern: `"return_list"` → `"returns_list"`

#### 6. PHP Schema Generator
Path: `fraisier/langgen/php/generator.php` or similar
- All query/mutation definitions
- Pattern: `"return_list"` → `"returns_list"`

#### 7. Kotlin Schema Generator
Path: `fraisier/langgen/kotlin/generator.kt` or similar
- All query/mutation definitions
- Pattern: `"return_list"` → `"returns_list"`

#### 8. C# Schema Generator
Path: `fraisier/langgen/csharp/generator.cs` or similar
- All query/mutation definitions
- Pattern: `"return_list"` → `"returns_list"`

#### 9. Rust Schema Generator
Path: `fraisier/langgen/rust/generator.rs` or similar
- All query/mutation definitions
- Pattern: `"return_list"` → `"returns_list"`

#### 10. JavaScript Schema Generator
Path: `fraisier/langgen/javascript/generator.js` or similar
- All query/mutation definitions
- Pattern: `"return_list"` → `"returns_list"`

#### 11. Ruby Schema Generator
Path: `fraisier/langgen/ruby/generator.rb` or similar
- All query/mutation definitions
- Pattern: `"return_list"` → `"returns_list"`

---

## Verification Strategy

### Step 1: Fix velocitybench_schemas.py
This is the canonical schema definition that all tests use. Fix must be done first.

### Step 2: Fix all 10 language generators
Each generator has a function that produces the schema JSON. Find and fix the field name.

### Step 3: Test each language independently
```bash
# For each language, test:
python3 tests/e2e/velocitybench_compilation_test.py
# Check Phase 2 output for that language
```

### Step 4: Verify all produce identical output
```bash
# Run full test to ensure all 10 languages compile identically
python3 tests/e2e/velocitybench_compilation_test.py
```

Expected output:
```
======================================================================
Phase 2: CLI Compilation E2E Test
======================================================================

Compiling Python       (Python decorators             )... ✅ schema.compiled.json
Compiling TypeScript   (TypeScript decorators         )... ✅ schema.compiled.json
Compiling Go           (Go struct tags                )... ✅ schema.compiled.json
Compiling Java         (Java annotations              )... ✅ schema.compiled.json
Compiling PHP          (PHP attributes                )... ✅ schema.compiled.json
Compiling Kotlin       (Kotlin data classes           )... ✅ schema.compiled.json
Compiling CSharp       (C# records                    )... ✅ schema.compiled.json
Compiling Rust         (Rust macros                   )... ✅ schema.compiled.json
Compiling JavaScript   (JavaScript decorators         )... ✅ schema.compiled.json
Compiling Ruby         (Ruby DSL                      )... ✅ schema.compiled.json

======================================================================
✅ ALL TIER 1A COMPILATION E2E TESTS PASSED!
✅ All 10 languages compile to identical canonical schema.compiled.json
======================================================================
```

---

## Additional Checks

### Check if there are other format issues
While fixing `return_list`, also verify:
- ✅ Field types use `"type"` not `"field_type"` - CORRECT
- ✅ Argument types use `"type"` not `"arg_type"` - CORRECT
- ✅ Query/mutation names are correct - CORRECT
- ✅ Nullable field is present - CORRECT
- ✅ Default values use `"default"` not `"defaultValue"` - VERIFY
- ✅ Arguments include `"nullable"` field - VERIFY

### Potential Secondary Issues
If you find other format mismatches during fixing, update this document.

---

## Commit Message Template

Once all fixes are complete:
```
fix(schema): Normalize schema field names for CLI compatibility

## Changes
- Fix "return_list" → "returns_list" in velocitybench_schemas.py
- Fix "return_list" → "returns_list" in all 10 language generators:
  - Python, TypeScript, Go, Java, PHP
  - Kotlin, C#, Rust, JavaScript, Ruby

## Verification
✅ All 10 languages now produce CLI-compatible schema format
✅ CLI compilation succeeds for all 10 languages
✅ All compiled schemas are identical (semantic equivalence proven)
✅ velocitybench_compilation_test.py Phase 2 passes completely
```

---

## Success Criteria

- [ ] velocitybench_schemas.py: All 8 `return_list` → `returns_list`
- [ ] Python generator: `return_list` → `returns_list`
- [ ] TypeScript generator: `return_list` → `returns_list`
- [ ] Go generator: `return_list` → `returns_list`
- [ ] Java generator: `return_list` → `returns_list`
- [ ] PHP generator: `return_list` → `returns_list`
- [ ] Kotlin generator: `return_list` → `returns_list`
- [ ] C# generator: `return_list` → `returns_list`
- [ ] Rust generator: `return_list` → `returns_list`
- [ ] JavaScript generator: `return_list` → `returns_list`
- [ ] Ruby generator: `return_list` → `returns_list`
- [ ] No other format issues detected
- [ ] All 10 languages compile successfully
- [ ] All compiled schemas are identical
- [ ] E2E tests Phase 2 passes completely
