# Phase 1: @requires/@provides Enforcement

**Duration**: 4 weeks (weeks 1-4)
**Lead Role**: Senior Rust Engineer
**Impact**: CRITICAL - Foundation for all federation directive enforcement
**Goal**: Implement field-level directive metadata, dependency graph construction, compile-time validation, and runtime enforcement

---

## Objective

Transform federation directives from "metadata stored but not enforced" to **fully enforced at compile time and runtime**. This is the foundation for all subsequent federation features.

### Key Insight
Field-level @requires/@provides are implicit contracts between subgraphs. Enforcement prevents entire categories of bugs at design time, not runtime.

---

## Success Criteria

### Must Have
- [ ] Field-level directive metadata stored in `FederatedType`
- [ ] Dependency graph construction with cycle detection
- [ ] Compile-time validation rejects invalid directives
- [ ] Runtime @requires enforcement prevents incorrect resolution
- [ ] @provides validation with helpful warnings
- [ ] 65+ new tests passing
- [ ] All existing tests still passing (1693+)
- [ ] Zero new clippy warnings

### Performance Targets
- [ ] @requires check overhead: <1ms per field
- [ ] Dependency graph construction: <100ms for 1000 types
- [ ] Compile-time validation: <200ms added to schema compilation

### Developer Experience
- [ ] Clear error messages for directive violations
- [ ] Helpful suggestions for common mistakes
- [ ] All error messages include trace context

---

## Architecture

### Field Federation Directives Structure

```rust
// NEW: Add to crates/fraiseql-core/src/federation/types.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldFederationDirectives {
    pub requires: Vec<FieldSelection>,      // Fields needed to resolve this field
    pub provides: Vec<FieldSelection>,      // Fields this resolver provides
    pub external: bool,                     // Is this field @external?
    pub shareable: bool,                    // Is this field @shareable?
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FieldSelection {
    pub path: Vec<String>,      // ["profile", "age"] for "profile.age"
    pub typename: String,       // The type this field belongs to
}

// Update existing FederatedType struct
pub struct FederatedType {
    // ... existing fields ...
    pub field_directives: HashMap<String, FieldFederationDirectives>,  // NEW
}
```

### Dependency Graph for Cycle Detection

```rust
// NEW: Create crates/fraiseql-core/src/federation/dependency_graph.rs

pub struct DependencyGraph {
    nodes: HashMap<String, DependencyNode>,
    edges: Vec<DependencyEdge>,
}

pub struct DependencyNode {
    pub typename: String,
    pub field: String,
    pub requires: Vec<FieldSelection>,
}

#[derive(Debug)]
pub struct DependencyEdge {
    pub from: (String, String),  // (Type, field)
    pub to: (String, String),    // (Type, field)
    pub reason: String,          // "required by" or "provided by"
}

impl DependencyGraph {
    pub fn build(metadata: &FederationMetadata) -> Result<Self>;
    pub fn detect_cycles(&self) -> Vec<Vec<String>>;
    pub fn topological_sort(&self) -> Result<Vec<String>>;
}
```

---

## TDD Cycles

### Cycle 1: Field-Level Metadata (Week 1)

#### RED - Write Failing Tests
Create: `crates/fraiseql-core/tests/federation_field_directives_test.rs`

```rust
#[test]
fn test_field_directive_storage() {
    let schema = parse_federated_schema(r#"
        @key(fields: "id")
        type User {
            id: ID!
            email: String
            orders: [Order!]! @requires(fields: "email")
        }
    "#);

    let user_type = schema.federation.get_type("User").unwrap();
    let orders_directives = user_type.field_directives.get("orders").unwrap();

    assert!(!orders_directives.requires.is_empty());
    assert_eq!(orders_directives.requires[0].path, vec!["email".to_string()]);
}

#[test]
fn test_field_directive_provides() {
    let schema = parse_federated_schema(r#"
        @key(fields: "id")
        @extends
        type Order {
            id: ID!
            total: Float! @external
            shippingEstimate: Float! @requires(fields: "total") @provides(fields: "weight")
        }
    "#);

    let order_type = schema.federation.get_type("Order").unwrap();
    let shipping_directives = order_type.field_directives.get("shippingEstimate").unwrap();

    assert!(!shipping_directives.provides.is_empty());
    assert_eq!(shipping_directives.provides[0].path, vec!["weight".to_string()]);
}

#[test]
fn test_shareable_field_directive() {
    let schema = parse_federated_schema(r#"
        @key(fields: "id")
        type Product {
            id: ID!
            name: String! @shareable
            price: Float!
        }
    "#);

    let product_type = schema.federation.get_type("Product").unwrap();
    let name_directives = product_type.field_directives.get("name").unwrap();

    assert!(name_directives.shareable);
}
```

#### GREEN - Implement Minimal Code
Modify: `crates/fraiseql-core/src/federation/types.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldFederationDirectives {
    pub requires: Vec<FieldSelection>,
    pub provides: Vec<FieldSelection>,
    pub external: bool,
    pub shareable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FieldSelection {
    pub path: Vec<String>,
    pub typename: String,
}

impl FederatedType {
    pub fn new(typename: String) -> Self {
        Self {
            typename,
            keys: Vec::new(),
            extends: false,
            external_fields: HashSet::new(),
            shareable_fields: HashSet::new(),
            field_directives: HashMap::new(),  // NEW
            resolvable: true,
        }
    }

    pub fn set_field_directives(
        &mut self,
        field_name: String,
        directives: FieldFederationDirectives,
    ) {
        self.field_directives.insert(field_name, directives);
    }

    pub fn get_field_directives(
        &self,
        field_name: &str,
    ) -> Option<&FieldFederationDirectives> {
        self.field_directives.get(field_name)
    }
}
```

Modify: `crates/fraiseql-cli/src/schema/parser.rs` - Parse @requires/@provides/@shareable directives

```rust
fn parse_field_directives(
    field: &graphql_parser::schema::Field<String>,
    federation_metadata: &mut FederationMetadata,
) -> FieldFederationDirectives {
    let mut directives = FieldFederationDirectives {
        requires: Vec::new(),
        provides: Vec::new(),
        external: false,
        shareable: false,
    };

    for directive in &field.directives {
        match directive.name.as_str() {
            "requires" => {
                directives.requires =
                    parse_field_selections(&directive.arguments);
            }
            "provides" => {
                directives.provides =
                    parse_field_selections(&directive.arguments);
            }
            "external" => {
                directives.external = true;
            }
            "shareable" => {
                directives.shareable = true;
            }
            _ => {}
        }
    }

    directives
}

fn parse_field_selections(
    args: &[(String, graphql_parser::schema::Value<String>)],
) -> Vec<FieldSelection> {
    args.iter()
        .filter_map(|(key, value)| {
            if key == "fields" {
                if let graphql_parser::schema::Value::String(s) = value {
                    // Parse "email" or "profile.age"
                    let path: Vec<String> = s
                        .split('.')
                        .map(|s| s.to_string())
                        .collect();
                    return Some(FieldSelection {
                        path,
                        typename: String::new(),
                    });
                }
            }
            None
        })
        .collect()
}
```

#### REFACTOR - Improve Design
- Extract `FieldDirectiveParser` into separate trait
- Add builder pattern for constructing directives
- Cache parsed directives in metadata

```rust
pub trait FieldDirectiveParser {
    fn parse_field_directives(
        &self,
        field: &Field,
        context: &ParsingContext,
    ) -> Result<FieldFederationDirectives>;
}

pub struct FieldDirectiveBuilder {
    directives: FieldFederationDirectives,
}

impl FieldDirectiveBuilder {
    pub fn new() -> Self {
        Self {
            directives: FieldFederationDirectives::default(),
        }
    }

    pub fn requires(mut self, fields: Vec<FieldSelection>) -> Self {
        self.directives.requires = fields;
        self
    }

    pub fn build(self) -> FieldFederationDirectives {
        self.directives
    }
}
```

#### CLEANUP - Final Polish
- Run `cargo clippy --all-targets --all-features` - fix all warnings
- Run `cargo test federation::field_directives::tests` - verify tests pass
- Commit:
  ```
  feat(federation): Add field-level directive metadata storage

  ## Changes
  - Add FieldFederationDirectives struct for @requires/@provides/@shareable
  - Parse field-level directives from GraphQL schema
  - Store directives in FederatedType.field_directives HashMap
  - Add builder pattern for directive construction

  ## Verification
  ✅ cargo check passes
  ✅ cargo clippy passes
  ✅ All federation field directive tests pass (5 new tests)
  ✅ All existing tests still pass (1693+)
  ```

**Expected**: 1693 → 1698 tests passing

---

### Cycle 2: Dependency Graph & Cycle Detection (Week 2)

#### RED - Write Failing Tests
Create: `crates/fraiseql-core/tests/federation_dependency_graph_test.rs`

```rust
#[test]
fn test_dependency_graph_build() {
    let metadata = create_test_federation_metadata(vec![
        ("User", vec![("orders", RequiresDirective::new("email"))]),
        ("Order", vec![("items", RequiresDirective::new("total"))]),
    ]);

    let graph = DependencyGraph::build(&metadata).unwrap();

    assert_eq!(graph.nodes.len(), 2);
    assert!(graph.nodes.contains_key("User.orders"));
    assert!(graph.nodes.contains_key("Order.items"));
}

#[test]
fn test_cycle_detection_no_cycles() {
    // User.orders requires email
    // Email has no requires
    let metadata = create_test_federation_metadata(vec![
        ("User", vec![("orders", RequiresDirective::new("email"))]),
    ]);

    let graph = DependencyGraph::build(&metadata).unwrap();
    let cycles = graph.detect_cycles();

    assert!(cycles.is_empty());
}

#[test]
fn test_cycle_detection_simple_cycle() {
    // User.orders requires Order.total
    // Order.total requires User.id (circular!)
    let metadata = FederationMetadata {
        types: vec![
            FederatedType {
                typename: "User".to_string(),
                field_directives: {
                    let mut map = HashMap::new();
                    map.insert("orders".to_string(), FieldFederationDirectives {
                        requires: vec![FieldSelection {
                            path: vec!["total".to_string()],
                            typename: "Order".to_string(),
                        }],
                        provides: Vec::new(),
                        external: false,
                        shareable: false,
                    });
                    map
                },
                ..Default::default()
            },
            FederatedType {
                typename: "Order".to_string(),
                field_directives: {
                    let mut map = HashMap::new();
                    map.insert("total".to_string(), FieldFederationDirectives {
                        requires: vec![FieldSelection {
                            path: vec!["id".to_string()],
                            typename: "User".to_string(),
                        }],
                        provides: Vec::new(),
                        external: false,
                        shareable: false,
                    });
                    map
                },
                ..Default::default()
            },
        ],
    };

    let graph = DependencyGraph::build(&metadata).unwrap();
    let cycles = graph.detect_cycles();

    assert!(!cycles.is_empty());
    assert_eq!(cycles[0], vec!["User.orders", "Order.total", "User.orders"]);
}

#[test]
fn test_topological_sort_valid_graph() {
    let metadata = create_test_federation_with_order(vec![
        ("User", vec![("orders", RequiresDirective::new("email"))]),
        ("Order", vec![("total", RequiresDirective::none())]),
    ]);

    let graph = DependencyGraph::build(&metadata).unwrap();
    let order = graph.topological_sort().unwrap();

    // Order.total should come before User.orders
    let total_idx = order.iter().position(|x| x == "Order.total").unwrap();
    let orders_idx = order.iter().position(|x| x == "User.orders").unwrap();
    assert!(total_idx < orders_idx);
}
```

#### GREEN - Implement Cycle Detection
Create: `crates/fraiseql-core/src/federation/dependency_graph.rs`

```rust
use std::collections::{HashMap, HashSet, VecDeque};

pub struct DependencyGraph {
    nodes: HashMap<String, DependencyNode>,
    edges: Vec<DependencyEdge>,
}

pub struct DependencyNode {
    pub id: String,  // "Type.field"
    pub typename: String,
    pub field: String,
    pub requires: Vec<FieldSelection>,
}

#[derive(Clone)]
pub struct DependencyEdge {
    pub from: String,
    pub to: String,
}

impl DependencyGraph {
    pub fn build(metadata: &FederationMetadata) -> Result<Self> {
        let mut nodes: HashMap<String, DependencyNode> = HashMap::new();
        let mut edges = Vec::new();

        // Build nodes
        for federated_type in &metadata.types {
            for (field_name, directives) in &federated_type.field_directives {
                let node_id = format!("{}.{}", federated_type.typename, field_name);

                nodes.insert(
                    node_id.clone(),
                    DependencyNode {
                        id: node_id,
                        typename: federated_type.typename.clone(),
                        field: field_name.clone(),
                        requires: directives.requires.clone(),
                    },
                );
            }
        }

        // Build edges (directed edges for @requires dependencies)
        for node in nodes.values() {
            for required in &node.requires {
                let target_id = format!("{}.{}", required.typename, required.path.join("."));
                edges.push(DependencyEdge {
                    from: node.id.clone(),
                    to: target_id,
                });
            }
        }

        Ok(Self { nodes, edges })
    }

    pub fn detect_cycles(&self) -> Vec<Vec<String>> {
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();
        let mut cycles = Vec::new();

        for node_id in self.nodes.keys() {
            if !visited.contains(node_id) {
                self.dfs_cycle_detection(
                    node_id,
                    &mut visited,
                    &mut rec_stack,
                    &mut cycles,
                    &mut vec![],
                );
            }
        }

        cycles
    }

    fn dfs_cycle_detection(
        &self,
        node_id: &str,
        visited: &mut HashSet<String>,
        rec_stack: &mut HashSet<String>,
        cycles: &mut Vec<Vec<String>>,
        path: &mut Vec<String>,
    ) {
        visited.insert(node_id.to_string());
        rec_stack.insert(node_id.to_string());
        path.push(node_id.to_string());

        for edge in &self.edges {
            if edge.from == node_id {
                let next = &edge.to;
                if !visited.contains(next) {
                    self.dfs_cycle_detection(next, visited, rec_stack, cycles, path);
                } else if rec_stack.contains(next) {
                    // Found a cycle
                    let cycle_start = path.iter().position(|x| x == next).unwrap();
                    let cycle: Vec<String> = path[cycle_start..].to_vec();
                    cycles.push(cycle);
                }
            }
        }

        rec_stack.remove(node_id);
        path.pop();
    }

    pub fn topological_sort(&self) -> Result<Vec<String>> {
        let cycles = self.detect_cycles();
        if !cycles.is_empty() {
            return Err(FraiseQLError::Validation {
                message: format!("Circular dependencies detected: {:?}", cycles),
                path: None,
            });
        }

        let mut in_degree: HashMap<String, usize> = HashMap::new();
        for node_id in self.nodes.keys() {
            in_degree.insert(node_id.clone(), 0);
        }

        for edge in &self.edges {
            *in_degree.get_mut(&edge.to).unwrap() += 1;
        }

        let mut queue: VecDeque<String> = in_degree
            .iter()
            .filter(|(_, degree)| **degree == 0)
            .map(|(id, _)| id.clone())
            .collect();

        let mut result = Vec::new();

        while let Some(node_id) = queue.pop_front() {
            result.push(node_id.clone());

            for edge in self.edges.iter().filter(|e| e.from == node_id) {
                let degree = in_degree.get_mut(&edge.to).unwrap();
                *degree -= 1;
                if *degree == 0 {
                    queue.push_back(edge.to.clone());
                }
            }
        }

        Ok(result)
    }
}
```

#### REFACTOR - Add More Detection Strategies
- Add path tracking to cycles for better error messages
- Add visualization helpers (for debugging)
- Cache topological sort result

#### CLEANUP - Polish
- `cargo clippy --all-targets` - fix warnings
- `cargo test federation::dependency_graph` - verify tests pass
- Commit:
  ```
  feat(federation): Implement dependency graph with cycle detection

  ## Changes
  - Build dependency graph from @requires directives
  - Detect circular dependencies using DFS
  - Implement topological sort for resolution order
  - Add comprehensive cycle detection tests

  ## Verification
  ✅ cargo check passes
  ✅ cargo clippy passes
  ✅ All dependency graph tests pass (5 new tests)
  ✅ All federation tests pass (10 total)
  ✅ All existing tests still pass (1693+)
  ```

**Expected**: 1698 → 1708 tests passing

---

### Cycle 3: Compile-Time Validation (Week 3)

#### RED - Write Failing Tests
Create: `crates/fraiseql-cli/tests/federation_validation_test.rs`

```rust
#[test]
fn test_validate_requires_nonexistent_field() {
    let schema_str = r#"
        @key(fields: "id")
        type User {
            id: ID!
            orders: [Order!]! @requires(fields: "nonexistent")
        }
    "#;

    let result = SchemaValidator::new().validate_federation(&parse_schema(schema_str));

    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(error.to_string().contains("nonexistent"));
}

#[test]
fn test_validate_provides_nonexistent_field() {
    let schema_str = r#"
        @key(fields: "id")
        type Order {
            id: ID!
            shippingEstimate: Float! @provides(fields: "nonexistent")
        }
    "#;

    let result = SchemaValidator::new().validate_federation(&parse_schema(schema_str));

    assert!(result.is_err());
}

#[test]
fn test_validate_external_field_must_be_on_extends_type() {
    let schema_str = r#"
        @key(fields: "id")
        type User {
            id: ID!
            email: String! @external
        }
    "#;

    let result = SchemaValidator::new().validate_federation(&parse_schema(schema_str));

    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(error.to_string().contains("@external"));
}

#[test]
fn test_validate_circular_requires() {
    let schema_str = r#"
        @key(fields: "id")
        type User {
            id: ID!
            orders: [Order!]! @requires(fields: "Order")
        }

        @key(fields: "id")
        type Order {
            id: ID!
            user: User! @requires(fields: "User")
        }
    "#;

    let result = SchemaValidator::new().validate_federation(&parse_schema(schema_str));

    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(error.to_string().contains("circular"));
}
```

#### GREEN - Implement Validation
Modify: `crates/fraiseql-cli/src/schema/validator.rs`

```rust
impl SchemaValidator {
    pub fn validate_federation(&self, schema: &IntermediateSchema) -> Result<()> {
        if schema.federation.is_none() {
            return Ok(());
        }

        let federation = schema.federation.as_ref().unwrap();

        // 1. Validate @key fields exist
        self.validate_key_fields(federation)?;

        // 2. Validate @external only on @extends types
        self.validate_external_fields(federation)?;

        // 3. Validate @requires fields exist and are accessible
        self.validate_requires_fields(federation, schema)?;

        // 4. Validate @provides fields exist
        self.validate_provides_fields(federation, schema)?;

        // 5. Validate no circular dependencies
        self.validate_no_cycles(federation)?;

        // 6. Validate @shareable conflicts
        self.validate_shareable_conflicts(federation)?;

        Ok(())
    }

    fn validate_requires_fields(
        &self,
        federation: &FederationMetadata,
        schema: &IntermediateSchema,
    ) -> Result<()> {
        for federated_type in &federation.types {
            for (field_name, directives) in &federated_type.field_directives {
                for required in &directives.requires {
                    // Check field exists
                    let type_def = schema
                        .types
                        .iter()
                        .find(|t| t.name == federated_type.typename)
                        .ok_or_else(|| FraiseQLError::Validation {
                            message: format!("Type {} not found", federated_type.typename),
                            path: Some(format!("{}.{}", federated_type.typename, field_name)),
                        })?;

                    // Check each component of the path exists
                    let mut current_type = type_def;
                    for (i, path_component) in required.path.iter().enumerate() {
                        let field = current_type
                            .fields
                            .iter()
                            .find(|f| &f.name == path_component)
                            .ok_or_else(|| FraiseQLError::Validation {
                                message: format!(
                                    "@requires references non-existent field: {}",
                                    required.path.join(".")
                                ),
                                path: Some(format!(
                                    "{}.{}: @requires(fields: \"{}\")",
                                    federated_type.typename,
                                    field_name,
                                    required.path.join(".")
                                )),
                            })?;

                        // If this is not the last component, it must be an object type
                        if i < required.path.len() - 1 {
                            current_type = schema
                                .types
                                .iter()
                                .find(|t| t.name == field.type_name)
                                .ok_or_else(|| FraiseQLError::Validation {
                                    message: format!(
                                        "@requires path component {} is not an object type",
                                        path_component
                                    ),
                                    path: None,
                                })?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn validate_no_cycles(&self, federation: &FederationMetadata) -> Result<()> {
        let graph = DependencyGraph::build(federation)?;
        let cycles = graph.detect_cycles();

        if !cycles.is_empty() {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "Circular @requires dependencies detected: {:?}",
                    cycles
                ),
                path: None,
            });
        }

        Ok(())
    }
}
```

#### REFACTOR - Improve Error Messages
- Add trace context to all validation errors
- Group related errors
- Provide suggestions for fixes

#### CLEANUP - Polish
- `cargo clippy --all-targets` - fix warnings
- `cargo test federation::validation` - verify tests pass
- Commit:
  ```
  feat(federation): Add compile-time validation for @requires/@provides directives

  ## Changes
  - Validate @requires references existing fields
  - Check for circular dependencies at compile time
  - Validate @provides field existence
  - Validate @external only on @extends types
  - Add comprehensive validation error messages

  ## Verification
  ✅ cargo check passes
  ✅ cargo clippy passes
  ✅ All validation tests pass (8 new tests)
  ✅ All federation tests pass (18 total)
  ✅ All existing tests still pass (1693+)
  ```

**Expected**: 1708 → 1716 tests passing

---

### Cycle 4: Runtime @requires Enforcement (Week 4)

#### RED - Write Failing Tests
Create: `crates/fraiseql-core/tests/federation_requires_runtime_test.rs`

```rust
#[tokio::test]
async fn test_requires_enforcement_database_resolver() {
    let resolver = setup_test_resolver().await;

    // Try to resolve User.orders without email
    let representation = EntityRepresentation::from_json(json!({
        "id": "123",
        // email is missing!
    }));

    let result = resolver
        .resolve_with_requires("User", &["orders"], &representation)
        .await;

    // Should fail because email is required
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("requires") || result
        .unwrap_err()
        .to_string()
        .contains("email"));
}

#[tokio::test]
async fn test_requires_enforcement_includes_required_fields() {
    let resolver = setup_test_resolver().await;

    // Resolve with email present
    let representation = EntityRepresentation::from_json(json!({
        "id": "123",
        "email": "test@example.com"
    }));

    let result = resolver
        .resolve_with_requires("User", &["orders"], &representation)
        .await
        .unwrap();

    // Should include both orders and email in result
    let result_json = json!(result);
    assert!(result_json["email"].is_string());
    assert!(result_json["orders"].is_array());
}

#[test]
fn test_requires_nested_field_path() {
    let schema = parse_federated_schema(r#"
        @key(fields: "id")
        @extends
        type Order {
            id: ID!
            user: User!
            shippingEstimate: Float! @requires(fields: "user.email")
        }
    "#);

    let order_type = schema.federation.get_type("Order").unwrap();
    let directives = order_type.field_directives.get("shippingEstimate").unwrap();

    assert_eq!(directives.requires[0].path, vec!["user".to_string(), "email".to_string()]);
}
```

#### GREEN - Implement Enforcement
Modify: `crates/fraiseql-core/src/federation/database_resolver.rs`

```rust
impl DatabaseEntityResolver {
    pub async fn resolve_with_requires(
        &self,
        typename: &str,
        fields: &[String],
        representation: &EntityRepresentation,
    ) -> Result<Vec<Value>> {
        // 1. Get field directives for all requested fields
        let mut required_fields = HashSet::new();

        for field in fields {
            if let Some(directives) = self.get_field_directives(typename, field) {
                // Add all required fields
                for required in &directives.requires {
                    let expanded = self.expand_field_path(&required.path);
                    required_fields.extend(expanded);
                }
            }
        }

        // 2. Validate representation has all required fields
        for required_field in &required_fields {
            if !representation.has_field(required_field) {
                return Err(FraiseQLError::RequiresMissing {
                    typename: typename.to_string(),
                    field: fields.join(","),
                    required_field: required_field.clone(),
                    trace_id: generate_trace_id(),
                    suggestion: format!(
                        "Ensure field '{}' is requested from the owning subgraph",
                        required_field
                    ),
                });
            }
        }

        // 3. Build SELECT list: requested + required + keys
        let mut select_fields: HashSet<String> = fields.iter().cloned().collect();
        select_fields.extend(required_fields);
        select_fields.extend(self.get_key_fields(typename));

        let mut select_fields: Vec<String> = select_fields.into_iter().collect();
        select_fields.sort();

        // 4. Build SQL query
        let sql = self.build_select_query(typename, &select_fields, representation)?;

        // 5. Execute query
        let results = self.pool.fetch_all(&sql).await?;

        // 6. Return mapped results
        Ok(self.map_results(typename, results))
    }

    fn expand_field_path(&self, path: &[String]) -> Vec<String> {
        // For nested paths like ["user", "email"], flatten to simple fields
        // Or return as-is depending on DB structure
        vec![path.join(".")]
    }

    fn get_field_directives(
        &self,
        typename: &str,
        field: &str,
    ) -> Option<&FieldFederationDirectives> {
        self.metadata
            .types
            .iter()
            .find(|t| t.typename == typename)?
            .field_directives
            .get(field)
    }

    fn get_key_fields(&self, typename: &str) -> Vec<String> {
        self.metadata
            .types
            .iter()
            .find(|t| t.typename == typename)
            .map(|t| {
                t.keys
                    .iter()
                    .flat_map(|k| k.fields.iter().cloned())
                    .collect()
            })
            .unwrap_or_default()
    }
}
```

Also implement for HTTP resolver:

```rust
// In crates/fraiseql-core/src/federation/http_resolver.rs

impl HttpEntityResolver {
    pub async fn resolve_with_requires(
        &self,
        typename: &str,
        fields: &[String],
        representations: &[EntityRepresentation],
    ) -> Result<Vec<Value>> {
        // 1. Compute required fields
        let required_fields = self.compute_required_fields(typename, fields)?;

        // 2. Validate all representations have required fields
        for repr in representations {
            for req_field in &required_fields {
                if !repr.has_field(req_field) {
                    return Err(FraiseQLError::RequiresMissing {
                        typename: typename.to_string(),
                        field: fields.join(","),
                        required_field: req_field.clone(),
                        trace_id: generate_trace_id(),
                        suggestion: format!(
                            "Ensure field '{}' is included in entity representations",
                            req_field
                        ),
                    });
                }
            }
        }

        // 3. Build _entities query with required fields
        let query = self.build_entities_query(typename, fields, &required_fields)?;

        // 4. Send request
        let response = self
            .client
            .post(&self.endpoint)
            .json(&json!({
                "query": query,
                "variables": {
                    "representations": representations
                }
            }))
            .send()
            .await?;

        Ok(response.json().await?)
    }
}
```

#### REFACTOR - Improve Efficiency
- Cache @requires directives
- Batch validate multiple fields
- Add metrics for enforcement

#### CLEANUP - Polish
- `cargo clippy --all-targets` - fix warnings
- `cargo nextest run federation` - verify all federation tests pass
- Commit:
  ```
  feat(federation): Implement runtime @requires enforcement

  ## Changes
  - Validate required fields present in entity representations
  - Augment SELECT queries to include required fields
  - Implement for both database and HTTP resolvers
  - Add trace context and helpful error messages

  ## Verification
  ✅ cargo check passes
  ✅ cargo clippy passes
  ✅ All runtime enforcement tests pass (5 new tests)
  ✅ All federation tests pass (23 total)
  ✅ All existing tests still pass (1693+)
  ```

**Expected**: 1716 → 1723 tests passing

---

## Phase Completion Criteria

By end of Week 4, verify:

- [ ] All 65+ tests passing (1723 total)
- [ ] Zero clippy warnings
- [ ] Field-level directives working end-to-end
- [ ] Compile-time validation blocks invalid schemas
- [ ] Runtime enforcement prevents data inconsistency
- [ ] Performance overhead <1ms per field
- [ ] All documentation updated

---

## Next Steps

After Phase 1 completion:
- [ ] Review Phase 1 code for architecture coherence
- [ ] Plan Phase 2: Schema Validation
- [ ] Archive Phase 1 work

---

## Cycle 1 Status: ✅ COMPLETE

All phases of Cycle 1 completed successfully:
- ✅ RED: Written 7 failing tests expecting FieldFederationDirectives
- ✅ GREEN: Implemented FieldFederationDirectives + FieldPathSelection structs
- ✅ GREEN: Added field_directives HashMap to FederatedType
- ✅ REFACTOR: Added helper methods to FederatedType (get/set/has/is methods)
- ✅ REFACTOR: Added FieldFederationDirectives builder pattern
- ✅ CLEANUP: Verified all tests passing, linting clean
- ✅ COMMIT: Phase 1, Cycle 1 GREEN and REFACTOR phases committed

**Results**:
- 7/7 new tests passing
- 94+ existing federation tests still passing
- Zero new warnings introduced
- ~300 lines of well-designed, tested code

**Commits**:
- `3afca9eb`: Phase 1, Cycle 1 GREEN - Add field-level directive metadata
- `76aee069`: Phase 1, Cycle 1 REFACTOR - Add helper methods and builders

---

**Phase Status**: Cycle 1 Complete, Ready for Cycle 2
**Created**: January 28, 2026
**Cycle 1 Completion**: January 28, 2026
**Target Completion**: February 25, 2026
