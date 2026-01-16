# FraiseQL v2 - Analytics Phase 8: Integration & Wiring

**Status**: ⏳ Not Started
**Priority**: High
**Estimated Effort**: 1-2 days
**Dependencies**: Phases 1-7 complete

---

## Objective

Wire all analytics modules into the compiler and runtime pipeline:
- Integrate fact table detection into schema compilation
- Add aggregate query routing to executor
- Update GraphQL type generation
- Add validation rules for analytics queries
- Create unified query interface

---

## Context

Phase 8 connects all individual analytics components into a cohesive system. After this phase:
- Fact tables are automatically detected during schema compilation
- Aggregate/window queries route to correct executors
- Type generation includes aggregate types
- Validation catches analytics-specific errors

**Current State**: Modules exist but aren't integrated into main pipeline
**Target State**: Seamless analytics query execution through standard GraphQL interface

---

## Files to Modify

### Compiler Integration
```
crates/fraiseql-core/src/compiler/mod.rs
crates/fraiseql-core/src/compiler/validator.rs
crates/fraiseql-core/src/compiler/codegen.rs
```

### Runtime Integration
```
crates/fraiseql-core/src/runtime/mod.rs
crates/fraiseql-core/src/runtime/executor.rs
crates/fraiseql-core/src/runtime/planner.rs
```

### Schema Updates
```
crates/fraiseql-core/src/schema/compiled.rs
```

---

## Implementation Steps

### Step 1: Integrate Fact Table Detection into Compiler

**Duration**: 2 hours

**Goal**: Automatically detect and introspect fact tables during schema compilation.

**Update `compiler/mod.rs`**:
```rust
pub struct Compiler {
    config: CompilerConfig,
    parser: SchemaParser,
    validator: SchemaValidator,
    lowering: SqlTemplateGenerator,
    codegen: CodeGenerator,
    fact_table_detector: FactTableDetector, // NEW
}

impl Compiler {
    pub fn compile(&self, schema_json: &str) -> Result<CompiledSchema> {
        // Phase 1: Parse JSON → Authoring IR
        let ir = self.parser.parse(schema_json)?;

        // Phase 2: Detect fact tables from database
        let fact_tables = self.detect_fact_tables(&ir)?;

        // Phase 3: Generate aggregate types for fact tables
        let aggregate_types = self.generate_aggregate_types(&fact_tables)?;

        // Phase 4: Merge aggregate types into IR
        let enriched_ir = self.merge_analytics_types(ir, aggregate_types)?;

        // Phase 5: Validate IR (including analytics validation)
        let validated_ir = self.validator.validate(enriched_ir)?;

        // Phase 6-7: Lower and codegen as before
        let sql_templates = self.lowering.generate(&validated_ir)?;
        let compiled = self.codegen.generate(&validated_ir, &sql_templates)?;

        Ok(compiled)
    }

    /// Detect fact tables from database connection
    fn detect_fact_tables(&self, ir: &AuthoringIR) -> Result<Vec<FactTableMetadata>> {
        let mut fact_tables = Vec::new();

        // Get database connection from config
        let db_url = self.config.database_url.as_ref()
            .ok_or_else(|| FraiseQLError::config("Database URL required for fact table detection"))?;

        // Connect to database
        let adapter = self.create_database_adapter(db_url)?;

        // Introspect all tables matching tf_* pattern
        let introspector = FactTableIntrospector::new(adapter);
        let tables = introspector.list_fact_tables()?;

        for table_name in tables {
            let metadata = introspector.introspect(&table_name)?;
            fact_tables.push(metadata);
        }

        Ok(fact_tables)
    }

    /// Generate GraphQL aggregate types from fact tables
    fn generate_aggregate_types(
        &self,
        fact_tables: &[FactTableMetadata],
    ) -> Result<Vec<GeneratedAggregateType>> {
        let mut types = Vec::new();

        for metadata in fact_tables {
            // Generate {Type}Aggregate result type
            let aggregate_type = AggregateTypeGenerator::generate_aggregate_type(
                &metadata.table_name,
                metadata,
                self.config.database_target,
            )?;

            // Generate {Type}GroupByInput
            let group_by_input = AggregateTypeGenerator::generate_group_by_input(
                &metadata.table_name,
                metadata,
            )?;

            // Generate {Type}HavingInput
            let having_input = AggregateTypeGenerator::generate_having_input(
                &metadata.table_name,
                metadata,
                self.config.database_target,
            )?;

            types.push(GeneratedAggregateType {
                aggregate_type,
                group_by_input,
                having_input,
                metadata: metadata.clone(),
            });
        }

        Ok(types)
    }

    /// Merge generated analytics types into IR
    fn merge_analytics_types(
        &self,
        mut ir: AuthoringIR,
        aggregate_types: Vec<GeneratedAggregateType>,
    ) -> Result<AuthoringIR> {
        for gen_type in aggregate_types {
            // Add aggregate result type
            ir.types.push(gen_type.aggregate_type);

            // Add input types
            ir.input_types.push(gen_type.group_by_input);
            ir.input_types.push(gen_type.having_input);

            // Add query field
            let query_field = self.generate_aggregate_query_field(&gen_type)?;
            ir.queries.push(query_field);
        }

        Ok(ir)
    }

    fn generate_aggregate_query_field(
        &self,
        gen_type: &GeneratedAggregateType,
    ) -> Result<IRQuery> {
        // Generate query field like:
        // sales_aggregate(
        //   groupBy: SalesGroupByInput!,
        //   where: SalesWhereInput,
        //   having: SalesHavingInput,
        //   orderBy: [OrderByInput!],
        //   limit: Int,
        //   offset: Int
        // ): [SalesAggregate!]!
        todo!("Generate aggregate query field")
    }
}

#[derive(Debug)]
struct GeneratedAggregateType {
    aggregate_type: IRType,
    group_by_input: IRInputType,
    having_input: IRInputType,
    metadata: FactTableMetadata,
}
```

**Tests**:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_fact_tables() {
        // Mock database with tf_sales, tf_orders
        let compiler = Compiler::new_with_test_db();
        let ir = AuthoringIR::default();

        let fact_tables = compiler.detect_fact_tables(&ir).unwrap();

        assert_eq!(fact_tables.len(), 2);
        assert_eq!(fact_tables[0].table_name, "tf_sales");
        assert_eq!(fact_tables[1].table_name, "tf_orders");
    }

    #[test]
    fn test_generate_aggregate_types() {
        let metadata = create_test_fact_table();
        let compiler = Compiler::new();

        let types = compiler.generate_aggregate_types(&[metadata]).unwrap();

        assert_eq!(types.len(), 1);
        assert_eq!(types[0].aggregate_type.name, "SalesAggregate");
        assert_eq!(types[0].group_by_input.name, "SalesGroupByInput");
        assert_eq!(types[0].having_input.name, "SalesHavingInput");
    }

    #[test]
    fn test_merge_analytics_types() {
        let mut ir = AuthoringIR::default();
        let aggregate_types = vec![create_test_generated_type()];
        let compiler = Compiler::new();

        let merged = compiler.merge_analytics_types(ir, aggregate_types).unwrap();

        assert!(merged.types.iter().any(|t| t.name == "SalesAggregate"));
        assert!(merged.input_types.iter().any(|t| t.name == "SalesGroupByInput"));
        assert!(merged.queries.iter().any(|q| q.name == "sales_aggregate"));
    }
}
```

**Verification**:
```bash
cargo test -p fraiseql-core compiler::tests::test_detect_fact_tables
cargo test -p fraiseql-core compiler::tests::test_generate_aggregate_types
```

---

### Step 2: Add Analytics Validation Rules

**Duration**: 3 hours

**Goal**: Add validation for aggregate/window queries.

**Update `compiler/validator.rs`**:
```rust
impl SchemaValidator {
    pub fn validate(&self, ir: AuthoringIR) -> Result<AuthoringIR> {
        // Existing validation...
        self.validate_types(&ir)?;
        self.validate_queries(&ir)?;

        // NEW: Validate analytics components
        self.validate_aggregate_types(&ir)?;
        self.validate_fact_table_queries(&ir)?;

        Ok(ir)
    }

    /// Validate aggregate type definitions
    fn validate_aggregate_types(&self, ir: &AuthoringIR) -> Result<()> {
        for type_def in &ir.types {
            if type_def.name.ends_with("Aggregate") {
                // Validate aggregate result type structure
                self.validate_aggregate_result_type(type_def)?;
            }
        }

        for input_type in &ir.input_types {
            if input_type.name.ends_with("GroupByInput") {
                self.validate_group_by_input(input_type)?;
            }
            if input_type.name.ends_with("HavingInput") {
                self.validate_having_input(input_type)?;
            }
        }

        Ok(())
    }

    fn validate_aggregate_result_type(&self, type_def: &IRType) -> Result<()> {
        // 1. Must have 'count' field
        if !type_def.fields.iter().any(|f| f.name == "count") {
            return Err(FraiseQLError::validation(
                format!("Aggregate type '{}' must have 'count' field", type_def.name)
            ));
        }

        // 2. Measure aggregate fields must have proper suffix
        for field in &type_def.fields {
            if field.name.ends_with("_sum")
                || field.name.ends_with("_avg")
                || field.name.ends_with("_min")
                || field.name.ends_with("_max")
                || field.name.ends_with("_stddev")
                || field.name.ends_with("_variance")
            {
                // Valid aggregate field
                continue;
            } else if field.name == "count" {
                // count field is valid
                continue;
            } else {
                // Dimension field - validate it's a simple type
                self.validate_dimension_field(field)?;
            }
        }

        Ok(())
    }

    fn validate_group_by_input(&self, input_type: &IRInputType) -> Result<()> {
        // All fields must be Boolean type
        for field in &input_type.fields {
            if field.type_name != "Boolean" {
                return Err(FraiseQLError::validation(
                    format!(
                        "GroupBy input field '{}' must be Boolean, got '{}'",
                        field.name, field.type_name
                    )
                ));
            }
        }
        Ok(())
    }

    fn validate_having_input(&self, input_type: &IRInputType) -> Result<()> {
        // All fields must have comparison operator suffixes
        for field in &input_type.fields {
            if !field.name.ends_with("_gt")
                && !field.name.ends_with("_gte")
                && !field.name.ends_with("_lt")
                && !field.name.ends_with("_lte")
                && !field.name.ends_with("_eq")
                && !field.name.ends_with("_ne")
            {
                return Err(FraiseQLError::validation(
                    format!(
                        "Having input field '{}' must have comparison suffix (_gt, _gte, etc.)",
                        field.name
                    )
                ));
            }
        }
        Ok(())
    }

    /// Validate fact table query definitions
    fn validate_fact_table_queries(&self, ir: &AuthoringIR) -> Result<()> {
        for query in &ir.queries {
            if query.name.ends_with("_aggregate") || query.name.ends_with("_window") {
                self.validate_analytics_query(query)?;
            }
        }
        Ok(())
    }

    fn validate_analytics_query(&self, query: &IRQuery) -> Result<()> {
        // 1. Must have groupBy parameter (for aggregates)
        if query.name.ends_with("_aggregate") {
            if !query.parameters.iter().any(|p| p.name == "groupBy") {
                return Err(FraiseQLError::validation(
                    format!("Aggregate query '{}' must have 'groupBy' parameter", query.name)
                ));
            }
        }

        // 2. May have where, having, orderBy, limit, offset parameters
        for param in &query.parameters {
            match param.name.as_str() {
                "groupBy" | "where" | "having" | "orderBy" | "limit" | "offset" => {
                    // Valid analytics parameter
                }
                _ => {
                    return Err(FraiseQLError::validation(
                        format!("Invalid analytics query parameter: '{}'", param.name)
                    ));
                }
            }
        }

        // 3. Return type must be list of aggregate type
        if !query.return_type.starts_with('[') || !query.return_type.contains("Aggregate") {
            return Err(FraiseQLError::validation(
                format!(
                    "Analytics query '{}' must return list of aggregate type, got '{}'",
                    query.name, query.return_type
                )
            ));
        }

        Ok(())
    }

    fn validate_dimension_field(&self, field: &IRField) -> Result<()> {
        // Dimension fields must be simple types (String, Int, Float, Date, etc.)
        let valid_types = ["String", "Int", "Float", "Boolean", "Date", "DateTime"];
        if !valid_types.contains(&field.type_name.as_str()) {
            return Err(FraiseQLError::validation(
                format!("Dimension field '{}' must be simple type, got '{}'", field.name, field.type_name)
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_aggregate_result_type_missing_count() {
        let type_def = IRType {
            name: "SalesAggregate".to_string(),
            fields: vec![
                IRField {
                    name: "revenue_sum".to_string(),
                    type_name: "Float".to_string(),
                    nullable: true,
                },
            ],
        };

        let validator = SchemaValidator::new();
        let result = validator.validate_aggregate_result_type(&type_def);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must have 'count' field"));
    }

    #[test]
    fn test_validate_group_by_input_non_boolean_field() {
        let input_type = IRInputType {
            name: "SalesGroupByInput".to_string(),
            fields: vec![
                IRInputField {
                    name: "category".to_string(),
                    type_name: "String".to_string(), // Should be Boolean
                    nullable: false,
                },
            ],
        };

        let validator = SchemaValidator::new();
        let result = validator.validate_group_by_input(&input_type);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must be Boolean"));
    }

    #[test]
    fn test_validate_having_input_invalid_suffix() {
        let input_type = IRInputType {
            name: "SalesHavingInput".to_string(),
            fields: vec![
                IRInputField {
                    name: "revenue_sum".to_string(), // Missing suffix
                    type_name: "Float".to_string(),
                    nullable: true,
                },
            ],
        };

        let validator = SchemaValidator::new();
        let result = validator.validate_having_input(&input_type);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("comparison suffix"));
    }
}
```

**Verification**:
```bash
cargo test -p fraiseql-core validator::tests
```

---

### Step 3: Integrate Analytics into Executor

**Duration**: 4 hours

**Goal**: Route aggregate/window queries to correct executors.

**Update `runtime/executor.rs`**:
```rust
impl Executor {
    /// Execute GraphQL query (dispatch to appropriate executor)
    pub async fn execute(&self, query: &str, variables: Option<&str>) -> Result<String> {
        // Parse GraphQL query
        let parsed = self.parse_query(query)?;

        // Determine query type
        match self.classify_query(&parsed)? {
            QueryType::Regular => self.execute_regular_query(&parsed, variables).await,
            QueryType::Aggregate => self.execute_aggregate_query_dispatch(&parsed, variables).await,
            QueryType::Window => self.execute_window_query_dispatch(&parsed, variables).await,
        }
    }

    /// Classify query type
    fn classify_query(&self, parsed: &ParsedQuery) -> Result<QueryType> {
        let query_name = &parsed.operation_name;

        if query_name.ends_with("_aggregate") {
            Ok(QueryType::Aggregate)
        } else if query_name.ends_with("_window") {
            Ok(QueryType::Window)
        } else {
            Ok(QueryType::Regular)
        }
    }

    /// Execute aggregate query (dispatch)
    async fn execute_aggregate_query_dispatch(
        &self,
        parsed: &ParsedQuery,
        variables: Option<&str>,
    ) -> Result<String> {
        // 1. Convert GraphQL variables to JSON
        let query_json = self.graphql_to_json(parsed, variables)?;

        // 2. Get fact table metadata
        let table_name = self.extract_table_name(&query_json)?;
        let metadata = self.get_fact_table_metadata(&table_name)?;

        // 3. Execute via aggregate executor
        self.execute_aggregate_query(&query_json, &parsed.operation_name, &metadata)
            .await
    }

    /// Execute window function query (dispatch)
    async fn execute_window_query_dispatch(
        &self,
        parsed: &ParsedQuery,
        variables: Option<&str>,
    ) -> Result<String> {
        // 1. Convert GraphQL variables to JSON
        let query_json = self.graphql_to_json(parsed, variables)?;

        // 2. Get fact table metadata
        let table_name = self.extract_table_name(&query_json)?;
        let metadata = self.get_fact_table_metadata(&table_name)?;

        // 3. Execute via window executor
        self.execute_window_query(&query_json, &parsed.operation_name, &metadata)
            .await
    }

    /// Get cached fact table metadata
    fn get_fact_table_metadata(&self, table_name: &str) -> Result<&FactTableMetadata> {
        self.schema
            .fact_tables
            .get(table_name)
            .ok_or_else(|| FraiseQLError::not_found(format!("Fact table '{}' not found", table_name)))
    }

    /// Convert GraphQL query + variables to internal JSON format
    fn graphql_to_json(&self, parsed: &ParsedQuery, variables: Option<&str>) -> Result<serde_json::Value> {
        // Parse variables
        let vars = if let Some(v) = variables {
            serde_json::from_str(v)?
        } else {
            serde_json::json!({})
        };

        // Convert GraphQL AST to JSON query format
        let mut query = serde_json::Map::new();
        query.insert("table".to_string(), serde_json::json!(parsed.table_name));

        // Extract groupBy, where, having, etc. from GraphQL arguments
        if let Some(group_by) = parsed.arguments.get("groupBy") {
            query.insert("groupBy".to_string(), group_by.clone());
        }

        if let Some(where_clause) = parsed.arguments.get("where") {
            query.insert("where".to_string(), where_clause.clone());
        }

        // ... extract other arguments

        Ok(serde_json::Value::Object(query))
    }
}

#[derive(Debug, Clone, PartialEq)]
enum QueryType {
    Regular,
    Aggregate,
    Window,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_regular_query() {
        let executor = create_test_executor();
        let parsed = ParsedQuery {
            operation_name: "getUser".to_string(),
            table_name: "v_user".to_string(),
            arguments: Default::default(),
        };

        let query_type = executor.classify_query(&parsed).unwrap();
        assert_eq!(query_type, QueryType::Regular);
    }

    #[test]
    fn test_classify_aggregate_query() {
        let executor = create_test_executor();
        let parsed = ParsedQuery {
            operation_name: "sales_aggregate".to_string(),
            table_name: "tf_sales".to_string(),
            arguments: Default::default(),
        };

        let query_type = executor.classify_query(&parsed).unwrap();
        assert_eq!(query_type, QueryType::Aggregate);
    }

    #[test]
    fn test_classify_window_query() {
        let executor = create_test_executor();
        let parsed = ParsedQuery {
            operation_name: "sales_window".to_string(),
            table_name: "tf_sales".to_string(),
            arguments: Default::default(),
        };

        let query_type = executor.classify_query(&parsed).unwrap();
        assert_eq!(query_type, QueryType::Window);
    }
}
```

**Verification**:
```bash
cargo test -p fraiseql-core runtime::executor
```

---

### Step 4: Update CompiledSchema with Analytics Metadata

**Duration**: 2 hours

**Update `schema/compiled.rs`**:
```rust
/// Compiled schema with pre-generated SQL templates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledSchema {
    /// Regular GraphQL types
    pub types: Vec<CompiledType>,

    /// GraphQL queries
    pub queries: Vec<CompiledQuery>,

    /// GraphQL mutations
    pub mutations: Vec<CompiledMutation>,

    /// Fact table metadata (NEW)
    pub fact_tables: HashMap<String, FactTableMetadata>,

    /// Pre-generated SQL templates
    pub sql_templates: HashMap<String, SqlTemplate>,

    /// Schema version
    pub version: String,
}

impl CompiledSchema {
    /// Register fact table metadata
    pub fn add_fact_table(&mut self, metadata: FactTableMetadata) {
        self.fact_tables.insert(metadata.table_name.clone(), metadata);
    }

    /// Get fact table by name
    pub fn get_fact_table(&self, name: &str) -> Option<&FactTableMetadata> {
        self.fact_tables.get(name)
    }

    /// List all fact tables
    pub fn list_fact_tables(&self) -> Vec<&str> {
        self.fact_tables.keys().map(|s| s.as_str()).collect()
    }
}
```

---

### Step 5: Create Integration Tests

**Duration**: 2 hours

Create `tests/integration/analytics_integration_test.rs`:

```rust
//! End-to-end analytics integration tests

use fraiseql_core::compiler::Compiler;
use fraiseql_core::runtime::Executor;
use std::sync::Arc;

#[tokio::test]
async fn test_compile_and_execute_aggregate_query() {
    // 1. Compile schema with fact tables
    let compiler = Compiler::new();
    let schema = compiler.compile(SCHEMA_WITH_FACT_TABLES).unwrap();

    // 2. Verify fact tables detected
    assert!(schema.fact_tables.contains_key("tf_sales"));

    // 3. Verify aggregate types generated
    assert!(schema.types.iter().any(|t| t.name == "SalesAggregate"));

    // 4. Create executor
    let db_adapter = create_test_db_adapter().await;
    let executor = Executor::new(schema, db_adapter);

    // 5. Execute aggregate query
    let query = r#"
        query {
            sales_aggregate(
                groupBy: { category: true, occurred_at_day: true }
                where: { occurred_at_gte: "2024-01-01" }
                having: { revenue_sum_gt: 1000 }
                orderBy: [{ field: "revenue_sum", direction: DESC }]
                limit: 10
            ) {
                category
                occurred_at_day
                count
                revenue_sum
                revenue_avg
            }
        }
    "#;

    let result = executor.execute(query, None).await.unwrap();
    let response: serde_json::Value = serde_json::from_str(&result).unwrap();

    // 6. Verify response
    assert!(response["data"]["sales_aggregate"].is_array());
    let results = response["data"]["sales_aggregate"].as_array().unwrap();
    assert!(results.len() <= 10);
}

#[tokio::test]
async fn test_validate_invalid_aggregate_query() {
    let compiler = Compiler::new();

    // Schema with invalid aggregate type (missing count field)
    let invalid_schema = r#"{
        "types": [{
            "name": "SalesAggregate",
            "fields": [
                {"name": "revenue_sum", "type": "Float"}
            ]
        }]
    }"#;

    let result = compiler.compile(invalid_schema);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("must have 'count' field"));
}

const SCHEMA_WITH_FACT_TABLES: &str = r#"{
    "types": [],
    "queries": [],
    "database_url": "postgresql://test:test@localhost:5433/test"
}"#;
```

**Verification**:
```bash
cargo test --test analytics_integration_test
```

---

## Acceptance Criteria

- [ ] Fact table detection integrated into compiler pipeline
- [ ] Aggregate types automatically generated during compilation
- [ ] Analytics validation rules added to validator
- [ ] Executor routes aggregate queries to aggregate executor
- [ ] Executor routes window queries to window executor
- [ ] CompiledSchema includes fact table metadata
- [ ] GraphQL queries dispatch correctly based on query name suffix
- [ ] End-to-end integration test passes
- [ ] All existing tests still pass
- [ ] Documentation updated

---

## Verification Commands

```bash
# Full test suite
cargo test -p fraiseql-core

# Integration tests
cargo test --test analytics_integration_test

# Lint
cargo clippy -p fraiseql-core -- -D warnings

# Check compilation
cargo check --all-targets
```

**Expected Output**:
```
running 680 tests (668 existing + 12 new)
test compiler::tests::test_detect_fact_tables ... ok
test compiler::tests::test_generate_aggregate_types ... ok
test validator::tests::test_validate_aggregate_result_type ... ok
test executor::tests::test_classify_aggregate_query ... ok
...
test result: ok. 680 passed; 0 failed; 0 ignored
```

---

## DO NOT

- ❌ Don't break existing regular query execution
- ❌ Don't skip validation for analytics queries
- ❌ Don't hardcode table names or types
- ❌ Don't forget to update CompiledSchema serialization
- ❌ Don't bypass fact table detection (auto-generate types)

---

## Notes

**Key Integration Points**:
1. Compiler: Detect fact tables → Generate types → Merge into IR
2. Validator: Validate aggregate types, GroupBy inputs, Having inputs
3. Executor: Classify query → Route to appropriate executor
4. Schema: Store fact table metadata for runtime lookup

**Backward Compatibility**:
- Regular queries must continue to work unchanged
- Analytics features are additive, not replacing existing functionality
- Schema without fact tables should compile without errors

**Performance**:
- Fact table detection runs once at compile time
- No runtime overhead for regular queries
- Metadata cached in CompiledSchema
