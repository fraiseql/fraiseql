# Phase 13: Advanced GraphQL Features & Performance Optimization

**Objective**: Implement advanced GraphQL spec features and performance optimizations to complete the Rust migration and achieve full GraphQL compliance.

**Current State**: Core GraphQL execution working in Rust with RBAC and security features

**Target State**: Full GraphQL spec compliance with advanced features and optimized performance

---

## Context

**Why This Phase Matters:**

- Complete GraphQL spec compliance for enterprise adoption
- Performance optimization for high-throughput scenarios
- Advanced features for complex query patterns
- Foundation for future GraphQL enhancements

**Dependencies:**

- Phase 9 (Unified Pipeline) ✅ Required
- Phase 11 (RBAC) ✅ Required
- Phase 12 (Security) ✅ Required

**Performance Target:**

- Query complexity analysis: <0.05ms
- Fragment resolution: <0.1ms
- Variable processing: <0.02ms
- Total advanced features overhead: <0.5ms

---

## Files to Modify/Create

### Rust Files (fraiseql_rs/src/graphql/)

- **fragments.rs** (NEW): Fragment cycle detection and resolution
- **variables.rs** (NEW): Advanced variable processing and validation
- **complexity.rs** (NEW): Query complexity analysis and cost calculation
- **directives.rs** (UPDATE): Full directive parsing with arguments
- **schema.rs** (UPDATE): Schema introspection capabilities

### Integration Files

- **fraiseql_rs/src/pipeline/unified.rs**: Integrate advanced GraphQL features
- **fraiseql_rs/src/graphql/mod.rs**: Export new modules
- **Cargo.toml**: Add any necessary dependencies

---

## Implementation Steps

### Step 1: Fragment Cycle Detection (fragments.rs)

```rust
//! Fragment cycle detection and advanced resolution.

use std::collections::{HashMap, HashSet};
use crate::graphql::types::{ParsedQuery, FragmentDefinition};

/// Fragment cycle detector using DFS with backtracking
pub struct FragmentValidator {
    fragments: HashMap<String, FragmentDefinition>,
}

impl FragmentValidator {
    pub fn new(fragments: HashMap<String, FragmentDefinition>) -> Self {
        Self { fragments }
    }

    /// Detect cycles in fragment dependencies
    pub fn detect_cycles(&self) -> Result<(), FragmentCycleError> {
        let mut visited = HashSet::new();
        let mut recursion_stack = HashSet::new();

        for fragment_name in self.fragments.keys() {
            if !visited.contains(fragment_name) {
                self.dfs_cycle_detection(fragment_name, &mut visited, &mut recursion_stack)?;
            }
        }

        Ok(())
    }

    fn dfs_cycle_detection(
        &self,
        fragment_name: &str,
        visited: &mut HashSet<String>,
        recursion_stack: &mut HashSet<String>,
    ) -> Result<(), FragmentCycleError> {
        visited.insert(fragment_name.to_string());
        recursion_stack.insert(fragment_name.to_string());

        if let Some(fragment) = self.fragments.get(fragment_name) {
            // Check for fragment spreads in this fragment
            for spread in &fragment.fragment_spreads {
                if !visited.contains(&spread.name) {
                    self.dfs_cycle_detection(&spread.name, visited, recursion_stack)?;
                } else if recursion_stack.contains(&spread.name) {
                    return Err(FragmentCycleError::CycleDetected {
                        cycle: self.build_cycle_path(fragment_name, &spread.name, recursion_stack),
                    });
                }
            }
        }

        recursion_stack.remove(fragment_name);
        Ok(())
    }

    fn build_cycle_path(&self, start: &str, current: &str, stack: &HashSet<String>) -> Vec<String> {
        let mut path = vec![start.to_string()];
        let mut found_start = false;

        for item in stack {
            if found_start || item == start {
                found_start = true;
                path.push(item.clone());
            }
            if item == current {
                break;
            }
        }

        path
    }
}

#[derive(Debug)]
pub enum FragmentCycleError {
    CycleDetected { cycle: Vec<String> },
    FragmentNotFound(String),
}
```

### Step 2: Advanced Variable Processing (variables.rs)

```rust
//! Advanced GraphQL variable processing and validation.

use std::collections::HashMap;
use serde_json::Value;
use crate::graphql::types::{VariableDefinition, VariableValue};

/// Variable processor with advanced validation
pub struct VariableProcessor;

impl VariableProcessor {
    /// Process and validate variables against definitions
    pub fn process_variables(
        variables: &HashMap<String, Value>,
        definitions: &[VariableDefinition],
    ) -> Result<HashMap<String, VariableValue>, VariableError> {
        let mut processed = HashMap::new();

        for def in definitions {
            let var_name = &def.name;

            // Check if variable is provided
            let value = if let Some(val) = variables.get(var_name) {
                self.validate_variable_value(val, &def.variable_type)?
            } else {
                // Check if variable has default value
                if let Some(default) = &def.default_value {
                    self.convert_json_to_variable_value(default.clone())?
                } else if def.variable_type.nullable {
                    VariableValue::Null
                } else {
                    return Err(VariableError::MissingRequiredVariable(var_name.clone()));
                }
            };

            processed.insert(var_name.clone(), value);
        }

        Ok(processed)
    }

    /// Validate variable value against GraphQL type
    fn validate_variable_value(
        &self,
        value: &Value,
        var_type: &crate::graphql::types::GraphQLType,
    ) -> Result<VariableValue, VariableError> {
        match var_type.kind {
            crate::graphql::types::TypeKind::Scalar(scalar_type) => {
                self.validate_scalar_value(value, scalar_type)
            }
            crate::graphql::types::TypeKind::List(item_type) => {
                self.validate_list_value(value, item_type)
            }
            crate::graphql::types::TypeKind::NonNull(inner_type) => {
                if value.is_null() {
                    return Err(VariableError::NullValueForNonNullType);
                }
                self.validate_variable_value(value, inner_type)
            }
            _ => self.convert_json_to_variable_value(value.clone()),
        }
    }

    fn validate_scalar_value(
        &self,
        value: &Value,
        scalar_type: &str,
    ) -> Result<VariableValue, VariableError> {
        match scalar_type {
            "String" => {
                if let Some(s) = value.as_str() {
                    Ok(VariableValue::String(s.to_string()))
                } else {
                    Err(VariableError::TypeMismatch {
                        expected: "String".to_string(),
                        actual: self.json_type_name(value),
                    })
                }
            }
            "Int" => {
                if let Some(n) = value.as_i64() {
                    Ok(VariableValue::Int(n as i32))
                } else {
                    Err(VariableError::TypeMismatch {
                        expected: "Int".to_string(),
                        actual: self.json_type_name(value),
                    })
                }
            }
            "Float" => {
                if let Some(n) = value.as_f64() {
                    Ok(VariableValue::Float(n))
                } else if let Some(n) = value.as_i64() {
                    Ok(VariableValue::Float(n as f64))
                } else {
                    Err(VariableError::TypeMismatch {
                        expected: "Float".to_string(),
                        actual: self.json_type_name(value),
                    })
                }
            }
            "Boolean" => {
                if let Some(b) = value.as_bool() {
                    Ok(VariableValue::Boolean(b))
                } else {
                    Err(VariableError::TypeMismatch {
                        expected: "Boolean".to_string(),
                        actual: self.json_type_name(value),
                    })
                }
            }
            "ID" => {
                if let Some(s) = value.as_str() {
                    Ok(VariableValue::String(s.to_string()))
                } else if let Some(n) = value.as_i64() {
                    Ok(VariableValue::String(n.to_string()))
                } else {
                    Err(VariableError::TypeMismatch {
                        expected: "ID".to_string(),
                        actual: self.json_type_name(value),
                    })
                }
            }
            _ => self.convert_json_to_variable_value(value.clone()),
        }
    }

    fn validate_list_value(
        &self,
        value: &Value,
        item_type: &crate::graphql::types::GraphQLType,
    ) -> Result<VariableValue, VariableError> {
        if let Some(arr) = value.as_array() {
            let mut validated_items = Vec::new();
            for item in arr {
                validated_items.push(self.validate_variable_value(item, item_type)?);
            }
            Ok(VariableValue::List(validated_items))
        } else {
            Err(VariableError::TypeMismatch {
                expected: "List".to_string(),
                actual: self.json_type_name(value),
            })
        }
    }

    fn convert_json_to_variable_value(&self, value: Value) -> Result<VariableValue, VariableError> {
        match value {
            Value::Null => Ok(VariableValue::Null),
            Value::Bool(b) => Ok(VariableValue::Boolean(b)),
            Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Ok(VariableValue::Int(i as i32))
                } else if let Some(f) = n.as_f64() {
                    Ok(VariableValue::Float(f))
                } else {
                    Err(VariableError::InvalidNumber)
                }
            }
            Value::String(s) => Ok(VariableValue::String(s)),
            Value::Array(arr) => {
                let mut items = Vec::new();
                for item in arr {
                    items.push(self.convert_json_to_variable_value(item)?);
                }
                Ok(VariableValue::List(items))
            }
            Value::Object(obj) => Ok(VariableValue::Object(obj)),
        }
    }

    fn json_type_name(&self, value: &Value) -> String {
        match value {
            Value::Null => "null".to_string(),
            Value::Bool(_) => "boolean".to_string(),
            Value::Number(_) => "number".to_string(),
            Value::String(_) => "string".to_string(),
            Value::Array(_) => "array".to_string(),
            Value::Object(_) => "object".to_string(),
        }
    }
}

#[derive(Debug)]
pub enum VariableError {
    MissingRequiredVariable(String),
    NullValueForNonNullType,
    TypeMismatch { expected: String, actual: String },
    InvalidNumber,
}

#[derive(Debug, Clone)]
pub enum VariableValue {
    Null,
    Int(i32),
    Float(f64),
    String(String),
    Boolean(bool),
    List(Vec<VariableValue>),
    Object(serde_json::Map<String, Value>),
}
```

### Step 3: Query Complexity Analysis (complexity.rs)

```rust
//! Query complexity analysis and cost calculation.

use std::collections::HashMap;
use crate::graphql::types::{ParsedQuery, FieldSelection};

/// Query complexity analyzer
pub struct ComplexityAnalyzer {
    schema_weights: HashMap<String, FieldWeight>,
}

#[derive(Debug, Clone)]
pub struct FieldWeight {
    pub base_cost: usize,
    pub multiplier_field: Option<String>, // Field that indicates result size
    pub max_multiplier: usize,
}

impl Default for ComplexityAnalyzer {
    fn default() -> Self {
        let mut schema_weights = HashMap::new();

        // Default weights for common patterns
        schema_weights.insert("users".to_string(), FieldWeight {
            base_cost: 10,
            multiplier_field: Some("limit".to_string()),
            max_multiplier: 100,
        });

        schema_weights.insert("posts".to_string(), FieldWeight {
            base_cost: 5,
            multiplier_field: Some("first".to_string()),
            max_multiplier: 50,
        });

        Self { schema_weights }
    }
}

impl ComplexityAnalyzer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Analyze query complexity
    pub fn analyze(&self, query: &ParsedQuery, variables: &HashMap<String, serde_json::Value>) -> ComplexityResult {
        let mut result = ComplexityResult::default();

        for selection in &query.selections {
            let field_complexity = self.calculate_field_complexity(selection, variables);
            result.total_complexity += field_complexity.complexity;
            result.field_complexities.push(field_complexity);
        }

        result
    }

    fn calculate_field_complexity(
        &self,
        selection: &FieldSelection,
        variables: &HashMap<String, serde_json::Value>,
    ) -> FieldComplexity {
        let mut complexity = 1; // Base complexity
        let mut depth = 1;

        // Get field weight from schema
        if let Some(weight) = self.schema_weights.get(&selection.name) {
            complexity = weight.base_cost;

            // Apply multiplier if present
            if let Some(multiplier_field) = &weight.multiplier_field {
                if let Some(var_value) = self.get_variable_value(multiplier_field, variables) {
                    if let Some(multiplier) = var_value.as_u64() {
                        let multiplier = multiplier.min(weight.max_multiplier as u64) as usize;
                        complexity *= multiplier;
                    }
                }
            }
        }

        // Recursively calculate nested complexity
        for nested in &selection.nested_fields {
            let nested_result = self.calculate_field_complexity(nested, variables);
            complexity += nested_result.complexity;
            depth = depth.max(nested_result.depth + 1);
        }

        FieldComplexity {
            field_name: selection.name.clone(),
            complexity,
            depth,
        }
    }

    fn get_variable_value<'a>(
        &self,
        var_name: &str,
        variables: &'a HashMap<String, serde_json::Value>,
    ) -> Option<&'a serde_json::Value> {
        variables.get(var_name)
    }
}

#[derive(Debug, Default)]
pub struct ComplexityResult {
    pub total_complexity: usize,
    pub field_complexities: Vec<FieldComplexity>,
}

impl ComplexityResult {
    pub fn exceeds_limit(&self, limit: usize) -> bool {
        self.total_complexity > limit
    }

    pub fn max_depth(&self) -> usize {
        self.field_complexities.iter()
            .map(|fc| fc.depth)
            .max()
            .unwrap_or(0)
    }
}

#[derive(Debug)]
pub struct FieldComplexity {
    pub field_name: String,
    pub complexity: usize,
    pub depth: usize,
}
```

### Step 4: Full Directive Parsing (directives.rs UPDATE)

```rust
//! Full GraphQL directive parsing with argument support.

use graphql_parser::query::{Directive, Value};
use crate::graphql::types::{ParsedQuery, FieldSelection};
use super::errors::{Result as GraphQLResult, GraphQLParseError};

/// Enhanced directive extractor with full argument parsing
pub struct DirectiveParser;

impl DirectiveParser {
    /// Extract directives with full argument parsing
    pub fn parse_directives(query: &str) -> GraphQLResult<ParsedDirectives> {
        // Parse GraphQL query with graphql-parser
        let document = graphql_parser::parse_query::<&str>(query)
            .map_err(|e| GraphQLParseError::ParseError(e.to_string()))?;

        let mut parsed = ParsedDirectives::default();

        // Extract directives from operations and fragments
        for definition in &document.definitions {
            match definition {
                graphql_parser::query::Definition::Operation(operation) => {
                    Self::parse_operation_directives(operation, &mut parsed)?;
                }
                graphql_parser::query::Definition::Fragment(fragment) => {
                    Self::parse_fragment_directives(fragment, &mut parsed)?;
                }
            }
        }

        Ok(parsed)
    }

    fn parse_operation_directives(
        operation: &graphql_parser::query::OperationDefinition,
        parsed: &mut ParsedDirectives,
    ) -> GraphQLResult<()> {
        // Parse directives on operation
        for directive in &operation.directives {
            let parsed_directive = Self::parse_directive(directive)?;
            parsed.operation_directives.push(parsed_directive);
        }

        // Parse directives on selection sets
        Self::parse_selection_set(&operation.selection_set, parsed)?;

        Ok(())
    }

    fn parse_fragment_directives(
        fragment: &graphql_parser::query::FragmentDefinition,
        parsed: &mut ParsedDirectives,
    ) -> GraphQLResult<()> {
        // Parse directives on fragment
        for directive in &fragment.directives {
            let parsed_directive = Self::parse_directive(directive)?;
            parsed.fragment_directives.push(parsed_directive);
        }

        // Parse directives on fragment selection sets
        Self::parse_selection_set(&fragment.selection_set, parsed)?;

        Ok(())
    }

    fn parse_selection_set(
        selection_set: &graphql_parser::query::SelectionSet,
        parsed: &mut ParsedDirectives,
    ) -> GraphQLResult<()> {
        for selection in &selection_set.items {
            match selection {
                graphql_parser::query::Selection::Field(field) => {
                    Self::parse_field_directives(field, parsed)?;
                }
                graphql_parser::query::Selection::FragmentSpread(spread) => {
                    for directive in &spread.directives {
                        let parsed_directive = Self::parse_directive(directive)?;
                        parsed.field_directives.push(FieldDirective {
                            field_path: spread.fragment_name.clone(),
                            directive: parsed_directive,
                        });
                    }
                }
                graphql_parser::query::Selection::InlineFragment(fragment) => {
                    for directive in &fragment.directives {
                        let parsed_directive = Self::parse_directive(directive)?;
                        parsed.inline_fragment_directives.push(parsed_directive);
                    }
                    Self::parse_selection_set(&fragment.selection_set, parsed)?;
                }
            }
        }

        Ok(())
    }

    fn parse_field_directives(
        field: &graphql_parser::query::Field,
        parsed: &mut ParsedDirectives,
    ) -> GraphQLResult<()> {
        let field_path = Self::build_field_path(field);

        for directive in &field.directives {
            let parsed_directive = Self::parse_directive(directive)?;
            parsed.field_directives.push(FieldDirective {
                field_path: field_path.clone(),
                directive: parsed_directive,
            });
        }

        // Recursively parse nested fields
        Self::parse_selection_set(&field.selection_set, parsed)?;

        Ok(())
    }

    fn parse_directive(directive: &Directive<&str>) -> GraphQLResult<ParsedDirective> {
        let mut arguments = HashMap::new();

        for (name, value) in &directive.arguments {
            let parsed_value = Self::parse_directive_value(value)?;
            arguments.insert(name.to_string(), parsed_value);
        }

        Ok(ParsedDirective {
            name: directive.name.to_string(),
            arguments,
        })
    }

    fn parse_directive_value(value: &Value<&str>) -> GraphQLResult<DirectiveValue> {
        match value {
            Value::Null => Ok(DirectiveValue::Null),
            Value::Int(i) => Ok(DirectiveValue::Int(*i)),
            Value::Float(f) => Ok(DirectiveValue::Float(*f)),
            Value::String(s) => Ok(DirectiveValue::String(s.to_string())),
            Value::Boolean(b) => Ok(DirectiveValue::Boolean(*b)),
            Value::Enum(e) => Ok(DirectiveValue::String(e.to_string())),
            Value::List(items) => {
                let mut parsed_items = Vec::new();
                for item in items {
                    parsed_items.push(Self::parse_directive_value(item)?);
                }
                Ok(DirectiveValue::List(parsed_items))
            }
            Value::Object(fields) => {
                let mut parsed_fields = HashMap::new();
                for (key, value) in fields {
                    parsed_fields.insert(key.to_string(), Self::parse_directive_value(value)?);
                }
                Ok(DirectiveValue::Object(parsed_fields))
            }
            Value::Variable(var) => Ok(DirectiveValue::Variable(var.to_string())),
        }
    }

    fn build_field_path(field: &graphql_parser::query::Field) -> String {
        let mut path = field.name.to_string();

        // Add alias if present
        if let Some(alias) = &field.alias {
            path = format!("{}:{}", alias, path);
        }

        path
    }
}

#[derive(Debug, Default)]
pub struct ParsedDirectives {
    pub operation_directives: Vec<ParsedDirective>,
    pub fragment_directives: Vec<ParsedDirective>,
    pub field_directives: Vec<FieldDirective>,
    pub inline_fragment_directives: Vec<ParsedDirective>,
}

#[derive(Debug)]
pub struct FieldDirective {
    pub field_path: String,
    pub directive: ParsedDirective,
}

#[derive(Debug)]
pub struct ParsedDirective {
    pub name: String,
    pub arguments: HashMap<String, DirectiveValue>,
}

#[derive(Debug, Clone)]
pub enum DirectiveValue {
    Null,
    Int(i64),
    Float(f64),
    String(String),
    Boolean(bool),
    List(Vec<DirectiveValue>),
    Object(HashMap<String, DirectiveValue>),
    Variable(String),
}
```

### Step 5: Integration with Pipeline (unified.rs UPDATE)

```rust
//! Integrate advanced GraphQL features into unified pipeline.

use super::graphql::{
    fragments::FragmentValidator,
    variables::VariableProcessor,
    complexity::ComplexityAnalyzer,
    directives::DirectiveParser,
};

// Add to GraphQLPipeline struct
pub struct GraphQLPipeline {
    // ... existing fields ...
    fragment_validator: Option<FragmentValidator>,
    variable_processor: VariableProcessor,
    complexity_analyzer: ComplexityAnalyzer,
    directive_parser: DirectiveParser,
}

impl GraphQLPipeline {
    pub fn with_advanced_features(mut self) -> Self {
        self.variable_processor = VariableProcessor;
        self.complexity_analyzer = ComplexityAnalyzer::new();
        self.directive_parser = DirectiveParser;
        self
    }

    pub fn with_fragment_validation(mut self, validator: FragmentValidator) -> Self {
        self.fragment_validator = Some(validator);
        self
    }

    pub async fn execute_with_advanced_features(
        &self,
        query_string: &str,
        variables: HashMap<String, serde_json::Value>,
        user_context: UserContext,
    ) -> Result<(Vec<u8>, Vec<(String, String)>)> {
        // Phase 13.1: Parse and validate fragments
        if let Some(validator) = &self.fragment_validator {
            validator.detect_cycles()
                .map_err(|e| GraphQLError::FragmentError(e.to_string()))?;
        }

        // Phase 13.2: Process and validate variables
        let parsed_query = crate::graphql::parser::parse_query(query_string)?;
        let processed_variables = self.variable_processor
            .process_variables(&variables, &parsed_query.variable_definitions)
            .map_err(|e| GraphQLError::VariableError(e.to_string()))?;

        // Phase 13.3: Analyze query complexity
        let complexity_result = self.complexity_analyzer
            .analyze(&parsed_query, &variables);

        if complexity_result.exceeds_limit(1000) {
            return Err(GraphQLError::ComplexityError {
                complexity: complexity_result.total_complexity,
                limit: 1000,
            });
        }

        // Phase 13.4: Parse directives
        let parsed_directives = self.directive_parser
            .parse_directives(query_string)
            .map_err(|e| GraphQLError::DirectiveError(e.to_string()))?;

        // Continue with existing pipeline...
        let response = self.execute_sync_advanced(
            query_string,
            processed_variables,
            user_context,
            &parsed_directives,
        )?;

        // Add complexity headers
        let mut headers = Vec::new();
        headers.push(("X-Query-Complexity".to_string(),
                     complexity_result.total_complexity.to_string()));
        headers.push(("X-Query-Depth".to_string(),
                     complexity_result.max_depth().to_string()));

        Ok((response, headers))
    }
}
```

---

## Verification Commands

### Build and Test

```bash
# Build with advanced features
cargo build --release --features advanced-graphql

# Run advanced GraphQL tests
cargo test --features advanced-graphql advanced_graphql::

# Integration tests
pytest tests/test_advanced_graphql.py -xvs

# Performance benchmarks
cargo bench --features advanced-graphql complexity_analysis
```

### Expected Performance

```
Fragment Cycle Detection: <0.01ms
Variable Processing: <0.02ms
Complexity Analysis: <0.05ms
Directive Parsing: <0.03ms

Total Advanced Features Overhead: <0.5ms
```

---

## Acceptance Criteria

**Functionality:**

- ✅ Fragment cycle detection with clear error messages
- ✅ Advanced variable processing and type validation
- ✅ Query complexity analysis with configurable limits
- ✅ Full directive parsing with argument support
- ✅ Integration with existing pipeline

**Performance:**

- ✅ Advanced features overhead <0.5ms total
- ✅ No impact on simple queries
- ✅ Efficient algorithms (O(n) complexity)
- ✅ Memory-safe implementations

**Compatibility:**

- ✅ Backwards compatible with existing queries
- ✅ Optional advanced features (can be disabled)
- ✅ Graceful degradation on errors

---

## Migration Strategy

**Week 1: Core Features**

- Fragment cycle detection
- Basic variable validation
- Complexity analysis foundation

**Week 2: Advanced Processing**

- Full directive parsing
- Enhanced variable processing
- Integration testing

**Week 3: Production**

- Performance optimization
- Monitoring integration
- Documentation updates

---

## Summary

**Phase 13 completes the GraphQL spec compliance** and advanced features:

- ✅ Fragment cycle detection (prevents infinite loops)
- ✅ Advanced variable processing (type validation)
- ✅ Query complexity analysis (DoS protection)
- ✅ Full directive parsing (metadata support)
- ✅ Performance optimization (sub-millisecond overhead)

**Combined with Phases 1-12:**

- Complete GraphQL spec compliance
- Enterprise-grade security (RBAC + advanced features)
- 10-100x performance improvement
- Production-ready GraphQL server
