# FraiseQL v2 GraphQL Specification Alignment Plan

## Executive Summary

**Current Compliance**: ~65-70%
**Target Compliance**: ~95% (June 2018 GraphQL Spec)
**Key Insight**: FraiseQL's compiled model can handle most GraphQL features at compile-time, not runtime.

---

## Architecture Understanding

FraiseQL v2 uses a **compiled execution model**:

```
                    COMPILE TIME                          RUNTIME
┌─────────────┐    ┌──────────────┐    ┌─────────────┐    ┌───────────────┐
│ Python/TS   │ →  │ schema.json  │ →  │ fraiseql    │ →  │ schema.       │
│ Decorators  │    │              │    │ compile     │    │ compiled.json │
└─────────────┘    └──────────────┘    └──────────────┘    └───────┬───────┘
                                                                    │
                                                           ┌────────▼────────┐
                                                           │ fraiseql-server │
                                                           │ (Rust runtime)  │
                                                           └─────────────────┘
```

**Key Principle**: GraphQL query features (fragments, aliases, directives) should be resolved during **schema compilation**, not at runtime. The runtime executes pre-optimized SQL.

---

## Gap Analysis & Resolution Strategy

### 1. FRAGMENTS (GraphQL Spec §2.9-2.10)

**Current State**: Not supported
**Resolution**: Compile-time expansion

#### Strategy

Fragments are **syntactic sugar** for field reuse. In a compiled model:

1. **Named Fragments** → Expanded inline during compilation
2. **Inline Fragments** → Type-based field selection at compile time

#### Implementation

**Add to `compiler/ir.rs`:**

```rust
/// Fragment definition for query reuse
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IRFragment {
    /// Fragment name
    pub name: String,
    /// Target type name
    pub on_type: String,
    /// Selected fields
    pub fields: Vec<String>,
    /// Nested fragments referenced
    pub spreads: Vec<String>,
}
```

**Add to `schema.json` format:**

```json
{
  "fragments": {
    "UserFields": {
      "on": "User",
      "fields": ["id", "name", "email"]
    },
    "PostFields": {
      "on": "Post",
      "fields": ["id", "title", "content"],
      "spreads": ["UserFields"]  // author fields
    }
  }
}
```

**Compilation Output**: Fragments are fully expanded in `schema.compiled.json`:

```json
{
  "queries": {
    "posts": {
      "projection": {
        "id": true,
        "title": true,
        "content": true,
        "author": {
          "id": true,
          "name": true,
          "email": true
        }
      }
    }
  }
}
```

**Files to Modify:**

- `crates/fraiseql-core/src/compiler/ir.rs` - Add `IRFragment`
- `crates/fraiseql-core/src/compiler/parser.rs` - Parse fragments from schema.json
- `crates/fraiseql-core/src/compiler/validator.rs` - Validate fragment cycles
- `crates/fraiseql-cli/src/commands/compile.rs` - Expand fragments during compilation

**Runtime Impact**: None - fragments are pre-expanded

---

### 2. FIELD ALIASES (GraphQL Spec §2.13)

**Current State**: Not supported
**Resolution**: Compile-time projection mapping

#### Strategy

Aliases rename fields in the output. In SQL:

- Source: `SELECT author.name FROM posts JOIN users AS author`
- GraphQL: `{ post { writer: author { name } } }`
- Output: `{ "post": { "writer": { "name": "..." } } }`

#### Implementation

**Add to projection model:**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldProjection {
    /// SQL column/relation name
    pub source: String,
    /// Output alias (if different from source)
    pub alias: Option<String>,
    /// Nested projections for relations
    pub nested: Option<HashMap<String, FieldProjection>>,
}
```

**schema.json with aliases:**

```json
{
  "queries": {
    "post": {
      "fields": {
        "writer": {
          "source": "author",
          "fields": ["id", "name"]
        }
      }
    }
  }
}
```

**Compilation**: Generates SQL with proper aliasing:

```sql
SELECT
  json_build_object(
    'id', p.id,
    'writer', json_build_object(  -- alias preserved in JSON
      'id', author.id,
      'name', author.name
    )
  )
FROM posts p
JOIN users author ON p.author_id = author.id
```

**Files to Modify:**

- `crates/fraiseql-core/src/schema/compiled.rs` - Add alias to `CompiledField`
- `crates/fraiseql-core/src/db/postgres/adapter.rs` - Use alias in JSON projection
- `crates/fraiseql-core/src/db/mysql/adapter.rs` - Same for MySQL
- `crates/fraiseql-core/src/db/sqlserver/adapter.rs` - Same for SQL Server

**Runtime Impact**: Minimal - alias is just a string in projection

---

### 3. DIRECTIVES (GraphQL Spec §2.12)

**Current State**: Not supported
**Resolution**: Hybrid compile-time/runtime

#### Strategy

| Directive | When Resolved | How |
|-----------|---------------|-----|
| `@deprecated` | Compile-time | Metadata only, no runtime effect |
| `@skip(if: true)` | Compile-time | Field removed from projection |
| `@skip(if: $var)` | Runtime | Conditional projection |
| `@include(if: false)` | Compile-time | Field removed from projection |
| `@include(if: $var)` | Runtime | Conditional projection |

#### Implementation

**Static Directives** (literal booleans):

```json
// schema.json
{
  "queries": {
    "user": {
      "fields": {
        "id": {},
        "email": { "skip": false },
        "internalId": { "skip": true }  // removed during compilation
      }
    }
  }
}
```

**Dynamic Directives** (variable-based):

```rust
// Add to runtime executor
pub struct ConditionalField {
    pub name: String,
    pub skip_if: Option<String>,    // Variable name
    pub include_if: Option<String>, // Variable name
}

impl Executor {
    fn apply_directives(
        &self,
        fields: &[ConditionalField],
        variables: &serde_json::Value,
    ) -> Vec<String> {
        fields.iter()
            .filter(|f| {
                if let Some(skip_var) = &f.skip_if {
                    if variables.get(skip_var) == Some(&serde_json::Value::Bool(true)) {
                        return false;
                    }
                }
                if let Some(include_var) = &f.include_if {
                    if variables.get(include_var) != Some(&serde_json::Value::Bool(true)) {
                        return false;
                    }
                }
                true
            })
            .map(|f| f.name.clone())
            .collect()
    }
}
```

**Files to Modify:**

- `crates/fraiseql-core/src/compiler/ir.rs` - Add directive support to `IRField`
- `crates/fraiseql-core/src/schema/compiled.rs` - Add `skip_if`, `include_if`
- `crates/fraiseql-core/src/runtime/executor.rs` - Apply runtime directives
- Python/TS authoring: Add `@deprecated`, `@skip`, `@include` decorators

**Runtime Impact**: Small - boolean checks on variables

---

### 4. STANDARD INTROSPECTION (GraphQL Spec §4.1-4.2)

**Current State**: Custom REST endpoint at `/introspection`
**Resolution**: Add GraphQL-native `__schema` and `__type` queries

#### Strategy

FraiseQL needs to respond to standard introspection queries:

```graphql
{
  __schema {
    types { name kind }
    queryType { name }
    mutationType { name }
  }
  __type(name: "User") {
    name
    fields { name type { name } }
  }
}
```

#### Implementation

**Option A: Compile-time generation (Recommended)**

Generate introspection response at compile time and serve statically:

```rust
// Generated during `fraiseql compile`
pub struct IntrospectionSchema {
    pub schema_json: String,  // Pre-built __schema response
    pub types: HashMap<String, String>,  // type name -> __type response
}

impl Executor {
    pub fn execute(&self, query: &str, variables: &Value) -> Result<String> {
        // Check for introspection
        if query.contains("__schema") {
            return Ok(self.introspection.schema_json.clone());
        }
        if let Some(type_name) = self.extract_type_query(query) {
            return Ok(self.introspection.types.get(&type_name)
                .cloned()
                .unwrap_or_else(|| r#"{"data":{"__type":null}}"#.to_string()));
        }

        // Normal query execution
        self.execute_compiled(query, variables).await
    }
}
```

**Option B: Runtime introspection (More flexible)**

Add introspection resolvers that query the compiled schema:

```rust
fn resolve_schema(&self) -> IntrospectionSchema {
    IntrospectionSchema {
        query_type: Some(IntrospectionType { name: "Query".to_string() }),
        mutation_type: if self.schema.mutations.is_empty() {
            None
        } else {
            Some(IntrospectionType { name: "Mutation".to_string() })
        },
        types: self.schema.types.iter()
            .map(|t| self.type_to_introspection(t))
            .collect(),
    }
}
```

**Files to Modify:**

- `crates/fraiseql-core/src/schema/introspection.rs` - NEW: Introspection types
- `crates/fraiseql-core/src/runtime/executor.rs` - Handle `__schema`/`__type`
- `crates/fraiseql-cli/src/commands/compile.rs` - Generate introspection JSON
- `crates/fraiseql-server/src/routes/graphql.rs` - Route introspection queries

**Apollo Studio/Sandbox Support:**
This enables Apollo DevTools, GraphiQL, Altair, etc. to work automatically.

---

### 5. HTTP GET SUPPORT (de-facto standard)

**Current State**: POST only
**Resolution**: Add GET handler

#### Implementation

```rust
// routes/graphql.rs

#[derive(Debug, Deserialize)]
pub struct GraphQLGetParams {
    pub query: String,
    #[serde(default)]
    pub variables: Option<String>,  // JSON-encoded
    #[serde(rename = "operationName")]
    pub operation_name: Option<String>,
}

pub async fn graphql_get_handler<A: DatabaseAdapter>(
    State(state): State<AppState<A>>,
    Query(params): Query<GraphQLGetParams>,
) -> Result<GraphQLResponse, ErrorResponse> {
    let variables = params.variables
        .map(|v| serde_json::from_str(&v))
        .transpose()
        .map_err(|e| ErrorResponse::from_error(
            GraphQLError::request(format!("Invalid variables JSON: {e}"))
        ))?;

    let request = GraphQLRequest {
        query: params.query,
        variables,
        operation_name: params.operation_name,
    };

    graphql_handler_inner(state, request).await
}

// In server.rs
.route(&config.graphql_path, get(graphql_get_handler::<A>).post(graphql_handler::<A>))
```

**Files to Modify:**

- `crates/fraiseql-server/src/routes/graphql.rs` - Add GET handler
- `crates/fraiseql-server/src/server.rs` - Register GET route

---

### 6. `__typename` SUPPORT (GraphQL Spec §2.7)

**Current State**: Partially implemented
**Resolution**: Always include in projection

#### Implementation

```rust
// In SQL generation
fn build_json_projection(&self, type_def: &TypeDefinition) -> String {
    let mut fields = vec![
        format!("'__typename', '{}'", type_def.name)  // Always first
    ];

    for field in &type_def.fields {
        fields.push(format!("'{}', {}", field.name, field.sql_column));
    }

    format!("json_build_object({})", fields.join(", "))
}
```

**Files to Modify:**

- `crates/fraiseql-core/src/db/postgres/adapter.rs` - Add `__typename`
- `crates/fraiseql-core/src/db/mysql/adapter.rs` - Same
- `crates/fraiseql-core/src/db/sqlserver/adapter.rs` - Same

---

### 7. APOLLO SANDBOX INTEGRATION

**Current State**: Not in v2
**Resolution**: Port from v1

#### Implementation

Already exists in v1 (`playground_tool: Literal["graphiql", "apollo-sandbox"]`).

```rust
// config.rs
pub enum PlaygroundTool {
    GraphiQL,
    ApolloSandbox,
}

// server.rs - GET endpoint for playground
pub async fn playground_handler(
    State(config): State<ServerConfig>,
) -> Html<String> {
    match config.playground_tool {
        PlaygroundTool::GraphiQL => Html(GRAPHIQL_HTML.to_string()),
        PlaygroundTool::ApolloSandbox => Html(APOLLO_SANDBOX_HTML.to_string()),
    }
}

const APOLLO_SANDBOX_HTML: &str = r#"
<!DOCTYPE html>
<html>
<head>
  <title>Apollo Sandbox</title>
</head>
<body>
  <div id="sandbox"></div>
  <script src="https://embeddable-sandbox.cdn.apollographql.com/_latest/embeddable-sandbox.umd.production.min.js"></script>
  <script>
    new window.EmbeddedSandbox({
      target: '#sandbox',
      initialEndpoint: window.location.origin + '/graphql',
    });
  </script>
</body>
</html>
"#;
```

**Files to Modify:**

- `crates/fraiseql-server/src/config.rs` - Add `playground_tool`
- `crates/fraiseql-server/src/routes/mod.rs` - Add playground route
- `crates/fraiseql-server/src/server.rs` - Register playground

---

## Implementation Phases

### Phase A: Core Query Language (Priority: HIGH)

| Task | Effort | Files |
|------|--------|-------|
| Field aliases | 2 days | schema/compiled.rs, db/*/adapter.rs |
| `__typename` always | 1 day | db/*/adapter.rs |
| HTTP GET support | 1 day | routes/graphql.rs, server.rs |

### Phase B: Fragments & Directives (Priority: MEDIUM)

| Task | Effort | Files |
|------|--------|-------|
| Fragment definitions | 2 days | compiler/ir.rs, compiler/parser.rs |
| Fragment expansion | 2 days | compiler/validator.rs, cli/compile.rs |
| Static directives | 2 days | compiler/ir.rs, schema/compiled.rs |
| Runtime directives | 2 days | runtime/executor.rs |

### Phase C: Introspection & Tooling (Priority: HIGH)

| Task | Effort | Files |
|------|--------|-------|
| Standard introspection | 3 days | schema/introspection.rs, runtime/executor.rs |
| Apollo Sandbox | 1 day | config.rs, routes/mod.rs, server.rs |
| GraphiQL update | 1 day | routes/playground.rs |

### Phase D: Validation & Compliance (Priority: MEDIUM)

| Task | Effort | Files |
|------|--------|-------|
| GraphQL parser integration | 3 days | NEW: graphql_parser module |
| Type validation | 2 days | validation.rs |
| Field existence validation | 1 day | validation.rs |

---

## Compliance Checklist

After implementation:

| Feature | Status | GraphQL Spec Section |
|---------|--------|---------------------|
| Field Selection | ✅ | §2.4 |
| Field Aliases | ✅ | §2.13 |
| Arguments | ✅ | §2.5 |
| Fragments | ✅ (schema.json) | §2.9-2.10 |
| Directives (@deprecated) | ✅ | §2.12 |
| Directives (@skip/@include) | 🔜 | §2.12 |
| Variables | ✅ | §2.6 |
| `__typename` | ✅ | §2.7 |
| Introspection | ✅ | §4.1-4.2 |
| POST requests | ✅ | HTTP spec |
| GET requests | ✅ | HTTP spec |
| Error format | ✅ | §7.1 |

### Phase Progress

- **Phase 1**: ✅ Complete (Core type system - types, queries, mutations, enums, input types)
- **Phase 2**: ✅ Complete (HTTP GET support, Apollo Sandbox, GraphiQL playground)
- **Phase 3**: ✅ Complete (Interface types with introspection)
- **Phase 4**: ✅ Complete (Union types with introspection)
- **Phase 5**: ✅ Complete (Introspection enhancements)
  - `includeDeprecated` filtering for `fields` and `enumValues`
  - `specifiedByURL` for custom scalars (DateTime, Date, Time, UUID, JSON)
  - Python SDK `@fraiseql.union` decorator
  - CLI union conversion from intermediate to compiled schema
- **Phase 6**: 🔜 Future (@skip/@include directive runtime execution)
- **Phase 7**: 🔜 Future (Validation & compliance)

### Phase 5 Implementation Details

**Rust Core (fraiseql-core)**:

- Added `specified_by_u_r_l` field to `IntrospectionType` struct
- Added `filter_deprecated_fields()` and `filter_deprecated_enum_values()` methods
- Added `filter_all_deprecated()` convenience method
- Updated `builtin_scalars()` to include spec URLs for custom scalars:
  - DateTime → <https://scalars.graphql.org/andimarek/date-time>
  - Date → <https://scalars.graphql.org/andimarek/local-date>
  - Time → <https://scalars.graphql.org/andimarek/local-time>
  - UUID → <https://tools.ietf.org/html/rfc4122>
  - JSON → <https://www.ecma-international.org/publications/files/ECMA-ST/ECMA-404.pdf>

**Rust CLI (fraiseql-cli)**:

- Added `IntermediateUnion` struct to intermediate.rs
- Added `unions` field to `IntermediateSchema`
- Added `convert_union()` method to converter.rs
- Added tests for union parsing and conversion

**Python SDK**:

- Added `@fraiseql.union(members=[...])` decorator
- Updated `SchemaRegistry.register_union()` method
- Updated `get_schema()` to include unions in output
- Exported `union` alias in `__init__.py`

---

## Testing Strategy

### Unit Tests

- Fragment expansion correctness
- Alias projection mapping
- Directive evaluation

### Integration Tests

- Apollo DevTools connection
- GraphiQL introspection
- Full query with fragments + aliases + directives

### Spec Compliance Tests

```rust
#[test]
fn test_introspection_schema_query() {
    let query = "{ __schema { queryType { name } } }";
    let result = executor.execute(query, &Value::Null).await.unwrap();
    let data: Value = serde_json::from_str(&result).unwrap();
    assert_eq!(data["data"]["__schema"]["queryType"]["name"], "Query");
}

#[test]
fn test_fragment_expansion() {
    let query = r#"
        query { user(id: 1) { ...UserFields } }
        fragment UserFields on User { id name email }
    "#;
    // Should work after fragment expansion
}
```

---

## Summary

FraiseQL v2's compiled model is **well-suited** for GraphQL compliance:

1. **Fragments** → Expand at compile-time (zero runtime cost)
2. **Aliases** → Bake into JSON projection (minimal cost)
3. **Directives** → Static: compile-time, Dynamic: runtime variable check
4. **Introspection** → Pre-generate at compile-time
5. **GET support** → Simple route addition

The architecture doesn't need fundamental changes - just additions to the compilation and projection layers.
