# Cycle 16-1: GREEN Phase - Core Federation Runtime Implementation

**Cycle**: 1 of 8
**Phase**: GREEN (Implement minimal code to pass tests)
**Duration**: ~4-5 days
**Focus**: Implement federation core types, entity resolver, and SDL generation

**Prerequisites**:
- RED phase complete with all failing tests
- Tests clearly define requirements
- Implementation path is clear

---

## Objective

Implement minimal federation core to pass all RED phase tests:
1. Federation types (EntityRepresentation, ResolutionStrategy, etc.)
2. `_entities` query handler
3. `_service` query & SDL generation
4. Entity representation parsing
5. Basic entity resolution (local only)

---

## Implementation Plan

### Part 1: Federation Types

**File**: `crates/fraiseql-core/src/federation/types.rs`

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::value::JsonValue;

/// Federation metadata attached to compiled schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationMetadata {
    pub enabled: bool,
    pub version: String, // "v2"
    pub types: Vec<FederatedType>,
}

/// Federated type definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederatedType {
    pub name: String,
    pub keys: Vec<KeyDirective>,
    pub is_extends: bool,
    pub external_fields: Vec<String>,
    pub shareable_fields: Vec<String>,
}

/// @key directive
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyDirective {
    pub fields: Vec<String>,
    pub resolvable: bool,
}

/// Entity representation from _entities query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityRepresentation {
    pub typename: String,
    pub key_fields: HashMap<String, JsonValue>,
    pub all_fields: HashMap<String, JsonValue>,
}

impl EntityRepresentation {
    /// Parse from _Any scalar input
    pub fn from_any(value: &JsonValue) -> Result<Self, String> {
        let obj = value.as_object()
            .ok_or_else(|| "Entity representation must be object".to_string())?;

        let typename = obj.get("__typename")
            .and_then(|v| v.as_string())
            .ok_or_else(|| "__typename field required".to_string())?
            .to_string();

        Ok(EntityRepresentation {
            typename,
            key_fields: HashMap::new(), // Populated by resolver
            all_fields: obj.clone(),
        })
    }
}

/// Resolution strategy for entity
#[derive(Debug, Clone)]
pub enum ResolutionStrategy {
    /// Entity is owned by this subgraph
    Local {
        view_name: String,
        key_columns: Vec<String>,
    },
    /// Resolve via direct database connection to another subgraph
    DirectDatabase {
        connection_string: String,
        key_columns: Vec<String>,
    },
    /// Resolve via HTTP to external subgraph
    Http {
        subgraph_url: String,
    },
}

/// Federation resolver
pub struct FederationResolver {
    metadata: FederationMetadata,
    strategy_cache: HashMap<String, ResolutionStrategy>,
}

impl FederationResolver {
    pub fn new(metadata: FederationMetadata) -> Self {
        Self {
            metadata,
            strategy_cache: HashMap::new(),
        }
    }
}
```

### Part 2: Module Entry Point

**File**: `crates/fraiseql-core/src/federation/mod.rs`

```rust
pub mod types;
pub mod entity_resolver;
pub mod service_sdl;
pub mod representation;

pub use types::*;
pub use entity_resolver::*;
pub use service_sdl::*;
pub use representation::*;

use crate::error::{FraiseQLError, Result};

/// Handle federation queries
pub async fn handle_federation_query(
    query_name: &str,
    args: &std::collections::HashMap<String, JsonValue>,
) -> Result<JsonValue> {
    match query_name {
        "_service" => handle_service_query().await,
        "_entities" => handle_entities_query(args).await,
        _ => Err(FraiseQLError::Validation {
            message: format!("Unknown federation query: {}", query_name),
            path: None,
        }),
    }
}

async fn handle_service_query() -> Result<JsonValue> {
    // Return federation SDL
    todo!()
}

async fn handle_entities_query(
    args: &std::collections::HashMap<String, JsonValue>,
) -> Result<JsonValue> {
    // Parse representations, resolve entities
    todo!()
}
```

### Part 3: Entity Representation Parsing

**File**: `crates/fraiseql-core/src/federation/representation.rs`

```rust
use super::types::EntityRepresentation;
use crate::value::JsonValue;
use std::collections::HashMap;

/// Parse entity representations from _entities input
pub fn parse_representations(
    input: &JsonValue,
    metadata: &super::FederationMetadata,
) -> Result<Vec<EntityRepresentation>, String> {
    let array = input.as_array()
        .ok_or_else(|| "Representations must be array".to_string())?;

    let mut reps = Vec::new();
    for item in array {
        let mut rep = EntityRepresentation::from_any(item)?;

        // Extract key fields based on metadata
        if let Some(fed_type) = metadata.types.iter().find(|t| t.name == rep.typename) {
            if let Some(key) = fed_type.keys.first() {
                for field in &key.fields {
                    if let Some(value) = rep.all_fields.get(field) {
                        rep.key_fields.insert(field.clone(), value.clone());
                    }
                }
            }
        }

        reps.push(rep);
    }

    Ok(reps)
}

/// Deduplicate entity representations by key
pub fn deduplicate_representations(
    reps: &[EntityRepresentation],
) -> Vec<EntityRepresentation> {
    let mut seen = std::collections::HashSet::new();
    reps.iter()
        .filter(|rep| {
            let key = format!("{}:{:?}", rep.typename, rep.key_fields);
            seen.insert(key)
        })
        .cloned()
        .collect()
}
```

### Part 4: Local Entity Resolution

**File**: `crates/fraiseql-core/src/federation/entity_resolver.rs`

```rust
use super::types::*;
use super::representation::parse_representations;
use crate::value::JsonValue;
use crate::runtime::DatabaseAdapter;
use std::sync::Arc;

pub struct EntityResolver {
    adapter: Arc<dyn DatabaseAdapter>,
    metadata: FederationMetadata,
}

impl EntityResolver {
    pub fn new(
        adapter: Arc<dyn DatabaseAdapter>,
        metadata: FederationMetadata,
    ) -> Self {
        Self { adapter, metadata }
    }

    /// Resolve entities using local database
    pub async fn resolve_local(
        &self,
        typename: &str,
        representations: &[EntityRepresentation],
        view_name: &str,
        selection: &crate::query::FieldSelection,
    ) -> Result<Vec<JsonValue>, String> {
        if representations.is_empty() {
            return Ok(Vec::new());
        }

        // Extract key values
        let first_rep = &representations[0];
        let key_fields: Vec<String> = first_rep.key_fields.keys().cloned().collect();

        if key_fields.is_empty() {
            return Err("No key fields found for entity".to_string());
        }

        // Build WHERE clause: key1 IN (...) AND key2 IN (...)
        let mut where_clause = String::from("WHERE ");
        for (i, key_field) in key_fields.iter().enumerate() {
            if i > 0 {
                where_clause.push_str(" AND ");
            }

            let key_values: Vec<String> = representations.iter()
                .filter_map(|rep| rep.key_fields.get(key_field))
                .map(|v| format!("'{}'", v))
                .collect();

            where_clause.push_str(&format!("{} IN ({})", key_field, key_values.join(", ")));
        }

        // Execute query
        let query = format!("SELECT * FROM {} {}", view_name, where_clause);

        // Get results from database adapter
        let results = self.adapter.execute_raw_query(&query).await
            .map_err(|e| format!("Database error: {}", e))?;

        // Project fields based on selection
        let projected = project_fields(&results, selection)?;

        Ok(projected)
    }
}

fn project_fields(
    results: &[JsonValue],
    selection: &crate::query::FieldSelection,
) -> Result<Vec<JsonValue>, String> {
    // Project requested fields from results
    // This is minimal implementation - just return as-is for now
    Ok(results.to_vec())
}
```

### Part 5: SDL Generation

**File**: `crates/fraiseql-core/src/federation/service_sdl.rs`

```rust
use super::types::FederationMetadata;

/// Generate federation-compliant SDL
pub fn generate_sdl(
    base_schema: &str,
    metadata: &FederationMetadata,
) -> String {
    if !metadata.enabled {
        return base_schema.to_string();
    }

    let mut sdl = String::from(base_schema);

    // Add federation schema directives
    let federation_schema = r#"
directive @key(fields: String!, resolvable: Boolean = true) repeatable on OBJECT
directive @extends on OBJECT
directive @external on FIELD_DEFINITION
directive @requires(fields: String!) on FIELD_DEFINITION
directive @provides(fields: String!) on FIELD_DEFINITION
directive @shareable on FIELD_DEFINITION | OBJECT
directive @link(url: String!, as: String, for: String, import: [String]) repeatable on SCHEMA

type _Service {
  sdl: String!
}

scalar _Any

union _Entity = "# + &metadata.types.iter()
        .map(|t| t.name.as_str())
        .collect::<Vec<_>>()
        .join(" | ") + r#"

extend type Query {
  _service: _Service!
  _entities(representations: [_Any!]!): [_Entity]!
}
"#;

    sdl.push_str("\n");
    sdl.push_str(federation_schema);

    // Add @key directive to types
    for fed_type in &metadata.types {
        // Modify schema to add @key directives
        // This is minimal - just append directives
        let key_fields = fed_type.keys.iter()
            .map(|k| format!("\"{}\"", k.fields.join(" ")))
            .collect::<Vec<_>>()
            .join(", ");

        let directive = format!("@key(fields: [{}])", key_fields);
        // In real impl, would modify the schema in-place
        // For now, append as comment
        sdl.push_str(&format!("# {} has keys: {}\n", fed_type.name, directive));
    }

    sdl
}
```

### Part 6: Integration with Executor

**File**: `crates/fraiseql-core/src/runtime/executor.rs` (modifications)

```rust
// Add to existing executor
use crate::federation;

impl<A: DatabaseAdapter> QueryExecutor<A> {
    pub async fn execute_query(
        &self,
        query: &str,
    ) -> Result<JsonValue> {
        // Parse query
        let parsed = self.parser.parse(query)?;

        // Check if federation query
        if self.is_federation_query(&parsed) {
            return self.execute_federation_query(&parsed).await;
        }

        // Normal query execution
        self.execute_normal_query(&parsed).await
    }

    fn is_federation_query(&self, query: &ParsedQuery) -> bool {
        query.root_selection.fields.iter()
            .any(|f| f.name == "_service" || f.name == "_entities")
    }

    async fn execute_federation_query(
        &self,
        query: &ParsedQuery,
    ) -> Result<JsonValue> {
        // Route to federation handler
        federation::handle_federation_query(
            &query.root_selection.fields[0].name,
            &query.variables,
        ).await
    }
}
```

### Part 7: Schema Integration

**File**: `crates/fraiseql-core/src/schema/compiled.rs` (modifications)

```rust
pub struct CompiledSchema {
    pub schema: String,
    pub types: HashMap<String, TypeDefinition>,
    pub federation: Option<federation::FederationMetadata>,
    // ... existing fields
}

impl CompiledSchema {
    pub fn with_federation(mut self, metadata: federation::FederationMetadata) -> Self {
        self.federation = Some(metadata);
        self
    }

    pub fn to_sdl_with_federation(&self) -> String {
        if let Some(ref federation_meta) = self.federation {
            federation::generate_sdl(&self.schema, federation_meta)
        } else {
            self.schema.clone()
        }
    }
}
```

---

## Implementation Checklist

### Core Types
- [ ] `FederationMetadata` type created
- [ ] `FederatedType` type created
- [ ] `KeyDirective` type created
- [ ] `EntityRepresentation` type created
- [ ] `ResolutionStrategy` enum created

### Entity Parsing
- [ ] `parse_representations` function works
- [ ] `deduplicate_representations` function works
- [ ] `_Any` scalar parsing works
- [ ] Type coercion handles basic types

### Local Resolution
- [ ] Local entity resolution queries database
- [ ] WHERE clause construction correct
- [ ] Multiple key support works
- [ ] Batching works (100+ entities)

### SDL Generation
- [ ] Federation directives added to SDL
- [ ] `_Entity` union includes correct types
- [ ] `_Any` scalar in SDL
- [ ] `_service` type in SDL
- [ ] `_entities` field in Query

### Integration
- [ ] Executor recognizes federation queries
- [ ] Schema includes federation metadata
- [ ] `_service` query handler works
- [ ] `_entities` query handler works

### Testing
- [ ] All RED phase unit tests pass
- [ ] All RED phase integration tests pass
- [ ] No compilation warnings
- [ ] Clippy checks pass

---

## Compilation & Testing

```bash
# Verify compilation
cargo check -p fraiseql-core

# Run federation tests
cargo test --test federation

# Expected output
test test_entities_query_recognized ... ok
test test_entities_representations_parsed ... ok
test test_entities_response_format ... ok
test test_entities_null_handling ... ok
test test_entities_batch_100 ... ok
test test_service_query_recognized ... ok
test test_sdl_includes_federation_directives ... ok
test test_sdl_includes_entity_union ... ok
test test_sdl_valid_graphql ... ok
test test_entity_representation_parse_typename ... ok
test test_entity_representation_key_fields ... ok
test test_entity_representation_null_values ... ok
test test_strategy_local_for_owned_entity ... ok
test test_strategy_caching ... ok
test test_batch_deduplication ... ok
test test_batch_latency ... ok
test test_federation_query_single_entity ... ok
test test_federation_query_batch_entities ... ok
test test_federation_service_sdl ... ok
test test_federation_partial_failure ... ok
test test_federation_spec_version_2 ... ok
test test_service_query_required_fields ... ok
test test_entities_query_required_signature ... ok
test test_any_scalar_required ... ok
test test_entity_union_required ... ok

test result: ok. 25 passed
```

---

## Performance Verification

```bash
# Run performance tests
cargo test --test federation test_batch_latency -- --nocapture

# Expected: 100 entities resolved in <8ms
```

---

## Next Phase: REFACTOR

After all tests pass:
1. Extract resolution logic into trait
2. Improve error handling
3. Optimize performance
4. Add comments for clarity
5. Continue to REFACTOR phase

---

## Files Modified/Created

### Created
- `crates/fraiseql-core/src/federation/types.rs`
- `crates/fraiseql-core/src/federation/mod.rs`
- `crates/fraiseql-core/src/federation/representation.rs`
- `crates/fraiseql-core/src/federation/entity_resolver.rs`
- `crates/fraiseql-core/src/federation/service_sdl.rs`

### Modified
- `crates/fraiseql-core/src/runtime/executor.rs`
- `crates/fraiseql-core/src/schema/compiled.rs`

---

**Status**: [~] In Progress (Implementing core)
**Next**: REFACTOR Phase - Extract traits and optimize
