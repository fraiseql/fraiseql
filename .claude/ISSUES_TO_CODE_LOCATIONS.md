# FraiseQL v2 - Issues Mapped to Exact Code Locations

Use this guide to navigate directly to each issue that needs fixing.

---

## Issue 1: QueryTraceBuilder Doctest Failure 🔴 CRITICAL

**Severity**: BLOCKING - Prevents `cargo test --doc`

### Location
```
crates/fraiseql-core/src/runtime/query_tracing.rs
  Lines: 57-78 (doctest example)
  Lines: 339 (warning - comparison)
```

### The Problem

**Doctest Error 1** (line 68):
```rust
let phase_result = builder.record_phase("compile", async {  // ❌ METHOD DOESN'T EXIST
    // Compilation logic here
    Ok(())
}).await;
```

**Error Message**:
```
error[E0599]: no method named `record_phase` found for struct `QueryTraceBuilder`
```

**What exists instead**:
- `record_phase_success(&mut self, phase_name: &str, duration_us: u64)`
- `record_phase_error(&mut self, phase_name: &str, duration_us: u64, error: &str)`

---

**Doctest Error 2** (line 74):
```rust
let trace = builder.finish(true, None)?;  // ❌ MISSING THIRD PARAMETER
```

**Error Message**:
```
error[E0061]: this method takes 3 arguments but 2 arguments were supplied
argument #3 of type `Option<usize>` is missing
```

**Actual signature**:
```rust
pub fn finish(
    self,
    success: bool,
    error: Option<&str>,
    result_count: Option<usize>,  // ← MISSING IN DOCTEST
) -> Result<QueryExecutionTrace>
```

---

### The Fix

**Step 1**: Open the file
```bash
code crates/fraiseql-core/src/runtime/query_tracing.rs
```

**Step 2**: Find the doctest (line 57-78)

**Step 3**: Replace the entire example block with:

```rust
/// # Example
///
/// ```rust
/// use fraiseql_core::runtime::query_tracing::QueryTraceBuilder;
///
/// let mut builder = QueryTraceBuilder::new("query_123", "{ user { id name } }");
///
/// // Record parse phase (2.5ms)
/// builder.record_phase_success("parse", 2500);
///
/// // Record validate phase (3ms)
/// builder.record_phase_success("validate", 3000);
///
/// // Record execute phase (7ms)
/// builder.record_phase_success("execute", 7000);
///
/// // Finalize trace with result count
/// let trace = builder.finish(true, None, Some(42))?;
/// assert_eq!(trace.success, true);
/// assert_eq!(trace.result_count, Some(42));
/// println!("Query took {} microseconds", trace.total_duration_us);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
```

**Step 4**: Fix warning at line 339

Find:
```rust
assert!(trace.total_duration_us >= 0);  // ⚠️ Useless comparison (u64 always >= 0)
```

Change to:
```rust
assert!(trace.total_duration_us > 0);  // Meaningful assertion: query took time
```

**Step 5**: Verify the fix

```bash
cargo test --doc -p fraiseql-core --lib
# Should show: test result: ok. 138 passed; 0 failed
```

---

### Also Fix Warning at Line 282

**File**: `crates/fraiseql-core/src/runtime/sql_logger.rs:282`

Find:
```rust
assert!(log.duration_us >= 0);  // ⚠️ Useless (u64 always >= 0)
```

Change to:
```rust
assert!(log.duration_us > 0);  // Meaningful: something was measured
// OR if no duration is okay:
// (just remove this assertion if it's not needed)
```

---

## Issue 2: GraphQL Parser Incomplete Features 🟡 HIGH

**Severity**: MEDIUM - Silently ignores features

### Location
```
crates/fraiseql-core/src/compiler/parser.rs
  Lines: 120-145 (warning checks + stubs)
```

### The Problem

**Location 1** (lines 122-133): Warning-only feature detection
```rust
if obj.contains_key("interfaces") {
    eprintln!("Warning: 'interfaces' feature in schema is not yet supported and will be ignored");
}
if obj.contains_key("unions") {
    eprintln!("Warning: 'unions' feature in schema is not yet supported and will be ignored");
}
if obj.contains_key("input_types") {
    eprintln!("Warning: 'input_types' feature in schema is not yet supported and will be ignored");
}
```

**Location 2** (lines 135-145): Empty stubs
```rust
Ok(AuthoringIR {
    types,
    enums,
    interfaces: Vec::new(),  // TODO: Parse interfaces from JSON
    unions: Vec::new(),      // TODO: Parse unions from JSON
    input_types: Vec::new(), // TODO: Parse input types from JSON
    queries,
    mutations,
    subscriptions,
    fact_tables,
})
```

### What Exists to Copy

Study these functions as templates:

**Location 1**: `parse_types()` function in same file
```rust
fn parse_types(&self, value: &Value) -> Result<Vec<IRType>> {
    let array = value.as_array().ok_or_else(|| FraiseQLError::Parse {
        message:  "types must be an array".to_string(),
        // ... error handling pattern
    })?;

    let mut types = Vec::new();
    for (idx, item) in array.iter().enumerate() {
        // Parse each item...
    }
    Ok(types)
}
```

**Location 2**: IR types in `compiler/ir.rs`
```rust
pub struct IRType {
    pub name: String,
    pub fields: Vec<IRField>,
    // ...
}
// Look for IRInterface, IRUnion, IRInputType structures
```

### The Fix

**Step 1**: Open the files
```bash
code crates/fraiseql-core/src/compiler/parser.rs
code crates/fraiseql-core/src/compiler/ir.rs  # For IR structures
```

**Step 2**: Implement `parse_interfaces()` after `parse_types()`

```rust
fn parse_interfaces(&self, value: &Value) -> Result<Vec<IRInterface>> {
    let array = value.as_array().ok_or_else(|| FraiseQLError::Parse {
        message:  "interfaces must be an array".to_string(),
        location: "interfaces".to_string(),
    })?;

    let mut interfaces = Vec::new();
    for (idx, item) in array.iter().enumerate() {
        let obj = item.as_object().ok_or_else(|| FraiseQLError::Parse {
            message:  format!("Interface {} must be an object", idx),
            location: format!("interfaces[{}]", idx),
        })?;

        let name = obj.get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FraiseQLError::Parse {
                message:  format!("Interface {} missing name", idx),
                location: format!("interfaces[{}].name", idx),
            })?
            .to_string();

        // Parse fields
        let fields = if let Some(fields_val) = obj.get("fields") {
            self.parse_fields(fields_val)?
        } else {
            Vec::new()
        };

        interfaces.push(IRInterface {
            name,
            fields,
        });
    }
    Ok(interfaces)
}
```

**Step 3**: Implement `parse_unions()` similarly

**Step 4**: Implement `parse_input_types()` similarly

**Step 5**: Update the `parse()` function to call new parsers

```rust
// In parse() function, replace:
interfaces: Vec::new(),  // TODO
unions: Vec::new(),      // TODO
input_types: Vec::new(), // TODO

// With:
interfaces: if let Some(interfaces_val) = obj.get("interfaces") {
    self.parse_interfaces(interfaces_val)?
} else {
    Vec::new()
},
unions: if let Some(unions_val) = obj.get("unions") {
    self.parse_unions(unions_val)?
} else {
    Vec::new()
},
input_types: if let Some(input_types_val) = obj.get("input_types") {
    self.parse_input_types(input_types_val)?
} else {
    Vec::new()
},
```

**Step 6**: Add comprehensive tests

File: `crates/fraiseql-core/tests/phase*_integration.rs` (or create new test file)

```rust
#[test]
fn test_parse_interface_basic() {
    let json = r#"{
        "interfaces": [
            {
                "name": "Node",
                "fields": [
                    {"name": "id", "type": "ID!"}
                ]
            }
        ]
    }"#;

    let parser = SchemaParser::new();
    let ir = parser.parse(json).unwrap();
    assert_eq!(ir.interfaces.len(), 1);
    assert_eq!(ir.interfaces[0].name, "Node");
}

#[test]
fn test_parse_union_basic() {
    // Similar test structure
}

#[test]
fn test_parse_input_type_basic() {
    // Similar test structure
}

// Add 25+ more test cases for edge cases
```

---

## Issue 3: HTTP Server Tests Missing 🟠 MEDIUM

**Severity**: MEDIUM - Integration layer untested

### Location
```
crates/fraiseql-server/src/server.rs
  Comment at top: // TODO: Add server tests
```

### What Needs Testing

```
GraphQL Endpoint Tests:
  ✗ POST /graphql with valid query → 200 OK
  ✗ GET /graphql with valid query → 200 OK
  ✗ Invalid query → 400 Bad Request
  ✗ Missing Content-Type → 400
  ✗ Large queries handled correctly

Middleware Tests:
  ✗ CORS headers present
  ✗ Bearer token validation
  ✗ Missing auth → 401
  ✗ Invalid token → 401
  ✗ OIDC flow integration

Endpoint Tests:
  ✗ GET /health → 200 OK
  ✗ GET /metrics → metrics data
  ✗ Rate limiting enforced
  ✗ Error responses formatted correctly

Error Handling:
  ✗ Database errors → 500
  ✗ Validation errors → 400
  ✗ Parse errors → 400
  ✗ Timeout errors → 504
  ✗ Authorization errors → 403
```

### The Fix

**Step 1**: Create test directory structure

```bash
mkdir -p crates/fraiseql-server/tests
touch crates/fraiseql-server/tests/integration_test.rs
```

**Step 2**: Add test infrastructure

```rust
// tests/integration_test.rs

use fraiseql_server::Server;
use reqwest::Client;

/// Test server setup helper
async fn setup_test_server() -> TestServer {
    // Create in-memory database
    // Create server instance
    // Return wrapper with convenience methods
}

#[tokio::test]
async fn test_graphql_post_valid_query() {
    let server = setup_test_server().await;

    let response = server.client()
        .post(&format!("{}/graphql", server.url()))
        .json(&serde_json::json!({
            "query": "{ __typename }"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
}

#[tokio::test]
async fn test_cors_headers_present() {
    let server = setup_test_server().await;

    let response = server.client()
        .options(&format!("{}/graphql", server.url()))
        .send()
        .await
        .unwrap();

    assert!(response
        .headers()
        .contains_key("access-control-allow-origin"));
}

// ... 20+ more tests
```

**Step 3**: Add Cargo.toml test dependencies

```toml
[dev-dependencies]
reqwest = { version = "0.11", features = ["json"] }
tokio = { version = "1", features = ["full"] }
serde_json = "1.0"
testcontainers = "0.15"  # For database
```

**Step 4**: Add all required tests

See `IMPLEMENTATION_PLAN_FIXES.md` Phase 4 for detailed list.

---

## Issue 4: Schema Optimizer Test Ignored 🟢 LOW

**Severity**: LOW - Unclear status

### Location
```
crates/fraiseql-cli/src/schema/optimizer.rs
  Test marker with #[ignore]
```

### Find the Ignored Test

```bash
grep -n "#\[ignore\]" crates/fraiseql-cli/src/schema/optimizer.rs
```

Output will show:
```
NNN: #[ignore = "TODO: Schema optimizer behavior changed - needs update (Phase 4+)"]
```

### The Fix Options

**Option A: Re-enable the test** (if optimizer is still relevant)

1. Find the test function
2. Remove `#[ignore]` attribute
3. Update test logic if needed
4. Verify `cargo test` passes

**Option B: Remove the test** (if optimizer is deprecated)

1. Find and comment out the test
2. Document why in a comment
3. Clean up unused optimizer code if not used elsewhere

**Option C: Defer** (if unclear)

1. Mark as `#[ignore = "Phase 5: Optimizer status TBD"]`
2. Document in PR why this needs future work

### Recommended: Option A (Re-enable)

```bash
# Step 1: Find test location
grep -B5 -A15 "test_schema_optimizer" crates/fraiseql-cli/src/schema/optimizer.rs

# Step 2: Update test to current API
# Fix any references to old structures/methods

# Step 3: Remove #[ignore]
# Change: #[ignore = "..."]
# To: #[test]

# Step 4: Verify
cargo test -p fraiseql-cli optimizer::tests
```

---

## Issue 5: Documentation Gaps 🟢 LOW

**Severity**: LOW - Quality improvement

### Location 1: Security Warning Missing

```
crates/fraiseql-core/src/db/traits.rs
  Method: execute_raw_query()
```

Find:
```rust
pub async fn execute_raw_query(&self, sql: &str) -> Result<...>;
```

Add doc comment:
```rust
/// Execute raw SQL query.
///
/// ⚠️ **Security Warning**: This method directly executes SQL without validation.
/// Only use with SQL generated internally or thoroughly validated.
/// Never pass untrusted user input directly to this method.
///
/// # Arguments
/// * `sql` - SQL query string (must be safe/trusted)
```

### Location 2: Error Context Trait Missing Example

```
crates/fraiseql-core/src/error.rs
  Trait: ErrorContext
```

Add example to trait documentation:
```rust
/// Extension trait for adding context to errors.
///
/// # Example
///
/// ```rust
/// use fraiseql_core::error::{ErrorContext, FraiseQLError};
///
/// async fn parse_query(input: &str) -> Result<Query> {
///     let parsed = serde_json::from_str(input)
///         .context("Failed to parse query JSON")?;
///     Ok(parsed)
/// }
/// ```
```

### Location 3: New Parser Features Documentation

In `crates/fraiseql-core/src/compiler/parser.rs`, add doc examples:

```rust
/// Parse GraphQL interface definitions from JSON.
///
/// # Example JSON Format
///
/// ```json
/// {
///   "interfaces": [
///     {
///       "name": "Node",
///       "fields": [
///         {"name": "id", "type": "ID!", "required": true}
///       ]
///     }
///   ]
/// }
/// ```
```

---

## Quick Navigation Commands

Copy-paste these to jump to each issue:

```bash
# Issue 1: Doctest
code crates/fraiseql-core/src/runtime/query_tracing.rs:61

# Issue 1: Warning
code crates/fraiseql-core/src/runtime/query_tracing.rs:339
code crates/fraiseql-core/src/runtime/sql_logger.rs:282

# Issue 2: Parser
code crates/fraiseql-core/src/compiler/parser.rs:138

# Issue 3: Server tests
code crates/fraiseql-server/src/server.rs:1

# Issue 4: Optimizer
grep -n "#\[ignore\]" crates/fraiseql-cli/src/schema/optimizer.rs

# Issue 5: Docs
grep -n "execute_raw_query" crates/fraiseql-core/src/db/traits.rs
```

---

## Verification After Each Fix

After fixing each issue, run:

```bash
# For Issue 1 (Doctest)
cargo test --doc -p fraiseql-core

# For Issues 2 (Parser)
cargo test -p fraiseql-core parser

# For Issue 3 (Server tests)
cargo test -p fraiseql-server

# For Issue 4 (Optimizer)
cargo test -p fraiseql-cli optimizer

# All together
cargo test && cargo test --doc && cargo clippy --all-targets --all-features -- -D warnings
```

---

## Success Criteria per Issue

| Issue | Success When |
|-------|--------------|
| Issue 1 | `cargo test --doc -p fraiseql-core` passes |
| Issue 2 | Parser tests pass + no eprintln warnings |
| Issue 3 | 25+ server tests pass, 85% coverage |
| Issue 4 | Optimizer test enabled + passes OR properly removed |
| Issue 5 | Documentation reviewed + approved in PR |

---

**Use this guide as a checklist while implementing each phase!**
