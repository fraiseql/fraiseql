//! Schema compiler for FraiseQL v2.
//!
//! # Overview
//!
//! The compiler transforms GraphQL schema definitions (from authoring-language decorators)
//! into optimized, executable `CompiledSchema` with pre-generated SQL templates.
//!
//! ## Compilation Pipeline
//!
//! ```text
//! JSON Schema (from decorators)
//!         ↓
//!    Parser (parse.rs)
//!         ↓ [Syntax validation]
//!
//! Authoring IR (ir.rs)
//!         ↓
//!   Validator (validator.rs)
//!         ↓ [Type checking, name validation]
//!
//! Lowering (lowering.rs)
//!         ↓ [Build optimized IR for code generation]
//!
//!   SQL Templates
//!         ↓ [Database-specific artifact]
//!
//!    Codegen (codegen.rs)
//!         ↓ [Generate runtime schema metadata]
//!
//! CompiledSchema JSON
//!         ↓
//! Ready for Runtime Execution
//! ```
//!
//! ## Design Principles
//!
//! ### 1. Separation of Concerns
//!
//! Schema definition (what queries exist?) is kept separate from execution
//! artifacts (how to execute them?). This allows:
//! - Different SQL generation strategies (optimize for OLTP vs OLAP)
//! - Database-specific optimizations without changing schema
//! - Reuse of schemas across backends
//! - Testing schema independently from SQL generation
//!
//! ### 2. Staged Compilation
//!
//! Each phase has a specific responsibility:
//! - **Parsing**: Convert JSON → AST, syntax validation
//! - **Validation**: Type checking, semantic validation, circular reference detection
//! - **Lowering**: Optimize IR, prepare for code generation
//! - **Codegen**: Generate runtime metadata and schema introspection data
//!
//! This separation makes the compiler maintainable, testable, and allows reuse of
//! phases for different purposes.
//!
//! ### 3. Immutable Intermediate State
//!
//! Each phase produces immutable data structures (AuthoringIR, CompiledSchema, etc.)
//! This ensures:
//! - Reproducible builds (same input = same output)
//! - Thread-safe processing
//! - Clear data flow and dependencies
//! - Easy debugging and verification
//!
//! # Phases
//!
//! 1. **Parse** (`parser.rs`): JSON schema → Authoring IR
//!    - Syntax validation
//!    - AST construction
//!
//! 2. **Validate** (`validator.rs`): Type checking and semantic validation
//!    - Field type binding
//!    - Circular reference detection
//!    - Auth rule validation
//!
//! 3. **Lower** (`lowering.rs`): IR optimization for execution
//!    - Fact table extraction
//!    - Query optimization
//!    - Template preparation
//!
//! 4. **Codegen** (`codegen.rs`): Generate CompiledSchema
//!    - Runtime metadata
//!    - Schema introspection data
//!    - Field mappings
//!
//! # Example
//!
//! ```rust,no_run
//! use fraiseql_core::compiler::Compiler;
//!
//! # fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create compiler
//! let compiler = Compiler::new();
//!
//! // Compile schema from JSON
//! let schema_json = r#"{
//!     "types": [...],
//!     "queries": [...]
//! }"#;
//!
//! let compiled = compiler.compile(schema_json)?;
//!
//! // Output CompiledSchema JSON
//! let output = compiled.to_json()?;
//! # Ok(())
//! # }
//! ```

pub mod aggregate_types;
pub mod aggregation;
mod codegen;
pub mod compilation_cache;
pub mod enum_validator;
pub mod fact_table;
pub mod ir;
mod lowering;
pub mod parser;
pub mod validator;
pub mod window_allowlist;
pub mod window_functions;

pub use aggregate_types::{AggregateType, AggregateTypeGenerator, GroupByInput, HavingInput};
pub use aggregation::{AggregationPlan, AggregationPlanner, AggregationRequest};
pub use codegen::CodeGenerator;
pub use compilation_cache::{CompilationCache, CompilationCacheConfig, CompilationCacheMetrics};
pub use enum_validator::EnumValidator;
pub use ir::{
    AuthoringIR, AutoParams, IRArgument, IRField, IRMutation, IRQuery, IRSubscription, IRType,
    MutationOperation,
};
pub use lowering::{DatabaseTarget, SqlTemplateGenerator};
pub use parser::SchemaParser;
pub use validator::{SchemaValidationError, SchemaValidator};
pub use window_functions::{WindowExecutionPlan, WindowFunction, WindowFunctionPlanner};

use crate::{error::Result, schema::CompiledSchema};

/// Compiler configuration.
#[derive(Debug, Clone)]
pub struct CompilerConfig {
    /// Target database for SQL generation.
    pub database_target: DatabaseTarget,

    /// Enable SQL template optimization.
    pub optimize_sql: bool,

    /// Strict mode: Fail on warnings.
    pub strict_mode: bool,

    /// Enable debug output.
    pub debug: bool,

    /// Database URL for fact table introspection (optional).
    /// If provided, compiler will auto-detect fact tables and generate aggregate types.
    pub database_url: Option<String>,
}

impl Default for CompilerConfig {
    fn default() -> Self {
        Self {
            database_target: DatabaseTarget::PostgreSQL,
            optimize_sql:    true,
            strict_mode:     false,
            debug:           false,
            database_url:    None,
        }
    }
}

/// Schema compiler.
///
/// Transforms authoring-time schema definitions into runtime-optimized
/// `CompiledSchema` with pre-generated SQL templates.
///
/// # Example
///
/// ```rust,no_run
/// use fraiseql_core::compiler::{Compiler, CompilerConfig, DatabaseTarget};
///
/// # fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let config = CompilerConfig {
///     database_target: DatabaseTarget::PostgreSQL,
///     optimize_sql: true,
///     ..Default::default()
/// };
///
/// let compiler = Compiler::with_config(config);
/// let compiled = compiler.compile(r#"{"types": [], "queries": []}"#)?;
/// # Ok(())
/// # }
/// ```
pub struct Compiler {
    config:    CompilerConfig,
    parser:    SchemaParser,
    validator: SchemaValidator,
    lowering:  SqlTemplateGenerator,
    codegen:   CodeGenerator,
}

impl Compiler {
    /// Create new compiler with default configuration.
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(CompilerConfig::default())
    }

    /// Create new compiler with custom configuration.
    #[must_use]
    pub const fn with_config(config: CompilerConfig) -> Self {
        Self {
            parser: SchemaParser::new(),
            validator: SchemaValidator::new(),
            lowering: SqlTemplateGenerator::new(config.database_target),
            codegen: CodeGenerator::new(config.optimize_sql),
            config,
        }
    }

    /// Compile schema from JSON.
    ///
    /// # Arguments
    ///
    /// * `schema_json` - JSON schema emitted by the authoring-language decorators
    ///
    /// # Returns
    ///
    /// Compiled schema with pre-generated SQL templates
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - JSON parsing fails
    /// - Schema validation fails
    /// - SQL template generation fails
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use fraiseql_core::compiler::Compiler;
    ///
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let compiler = Compiler::new();
    /// let schema_json = r#"{"types": [], "queries": []}"#;
    /// let compiled = compiler.compile(schema_json)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn compile(&self, schema_json: &str) -> Result<CompiledSchema> {
        // Parse JSON → Authoring IR
        tracing::debug!("Parsing schema...");
        let ir = self.parser.parse(schema_json)?;

        // Validate IR
        tracing::debug!("Validating schema...");
        let validated_ir = self.validator.validate(ir)?;

        // Lower IR → SQL templates (validates SQL generation; templates currently unused by
        // codegen)
        tracing::debug!("Generating SQL templates...");
        let _sql_templates = self.lowering.generate(&validated_ir)?;

        // Codegen: IR → CompiledSchema
        tracing::debug!("Generating CompiledSchema...");
        let compiled = self.codegen.generate(&validated_ir)?;

        // Note: Fact table metadata will be added by external tools or
        // through explicit API calls (e.g., from authoring-language decorators)

        tracing::debug!("Compilation complete!");

        Ok(compiled)
    }

    /// Get compiler configuration.
    #[must_use]
    pub const fn config(&self) -> &CompilerConfig {
        &self.config
    }
}

impl Default for Compiler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use super::*;
    use crate::error::FraiseQLError;

    // ── EP-1: Parse error paths ───────────────────────────────────────────────

    #[test]
    fn test_compile_rejects_invalid_json() {
        let err = Compiler::new().compile("not json").unwrap_err();
        assert!(matches!(err, FraiseQLError::Parse { .. }), "got: {err:?}");
    }

    #[test]
    fn test_compile_rejects_non_object_schema() {
        let err = Compiler::new().compile(r#"["not", "an", "object"]"#).unwrap_err();
        assert!(matches!(err, FraiseQLError::Parse { .. }), "got: {err:?}");
    }

    #[test]
    fn test_compile_rejects_types_not_array() {
        let err = Compiler::new().compile(r#"{"types": "wrong"}"#).unwrap_err();
        assert!(matches!(err, FraiseQLError::Parse { .. }), "got: {err:?}");
    }

    #[test]
    fn test_compile_rejects_type_without_name() {
        // A type object that is missing the required "name" field.
        let schema = r#"{"types": [{"fields": []}]}"#;
        let err = Compiler::new().compile(schema).unwrap_err();
        assert!(matches!(err, FraiseQLError::Parse { .. }), "got: {err:?}");
    }

    // ── EP-2: Validation error paths ─────────────────────────────────────────

    #[test]
    fn test_compile_rejects_unknown_field_type() {
        let schema = r#"{"types": [{"name": "User", "fields": [
            {"name": "id", "type": "NonExistentType"}
        ]}]}"#;
        let err = Compiler::new().compile(schema).unwrap_err();
        assert!(matches!(err, FraiseQLError::Validation { .. }), "got: {err:?}");
    }

    #[test]
    fn test_compile_rejects_query_with_unknown_return_type() {
        // "User" is not defined in types, so the query return type is unknown.
        let schema = r#"{"types": [], "queries": [
            {"name": "getUser", "return_type": "User", "returns_list": false}
        ]}"#;
        let err = Compiler::new().compile(schema).unwrap_err();
        assert!(matches!(err, FraiseQLError::Validation { .. }), "got: {err:?}");
        if let FraiseQLError::Validation { message, .. } = err {
            assert!(
                message.contains("User"),
                "error message should name the unknown type: {message}"
            );
        }
    }

    #[test]
    fn test_compiler_new() {
        let compiler = Compiler::new();
        assert_eq!(compiler.config.database_target, DatabaseTarget::PostgreSQL);
        assert!(compiler.config.optimize_sql);
    }

    #[test]
    fn test_compiler_with_config() {
        let config = CompilerConfig {
            database_target: DatabaseTarget::MySQL,
            optimize_sql:    false,
            strict_mode:     true,
            debug:           true,
            database_url:    None,
        };

        let compiler = Compiler::with_config(config);
        assert_eq!(compiler.config.database_target, DatabaseTarget::MySQL);
        assert!(!compiler.config.optimize_sql);
        assert!(compiler.config.strict_mode);
        assert!(compiler.config.debug);
    }

    #[test]
    fn test_default_config() {
        let config = CompilerConfig::default();
        assert_eq!(config.database_target, DatabaseTarget::PostgreSQL);
        assert!(config.optimize_sql);
        assert!(!config.strict_mode);
        assert!(!config.debug);
        assert!(config.database_url.is_none());
    }

    #[test]
    fn test_compile_schema_with_fact_tables() {
        let compiler = Compiler::new();
        let schema_json = r#"{
            "types": [],
            "queries": [],
            "mutations": []
        }"#;

        let compiled = compiler
            .compile(schema_json)
            .unwrap_or_else(|e| panic!("expected Ok: {e}"));
        assert_eq!(compiled.fact_tables.len(), 0);
    }

    #[test]
    fn test_compiled_schema_fact_table_operations() {
        use crate::compiler::fact_table::{DimensionColumn, FactTableMetadata};

        let mut schema = CompiledSchema::new();

        let metadata = FactTableMetadata {
            table_name:           "tf_sales".to_string(),
            measures:             vec![],
            dimensions:           DimensionColumn {
                name:  "data".to_string(),
                paths: vec![],
            },
            denormalized_filters: vec![],
            calendar_dimensions:  vec![],
        };

        schema.add_fact_table("tf_sales".to_string(), metadata.clone());

        assert!(schema.has_fact_tables());

        let tables = schema.list_fact_tables();
        assert_eq!(tables.len(), 1);
        assert!(tables.contains(&"tf_sales"));

        let retrieved = schema.get_fact_table("tf_sales");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), &metadata);

        assert!(schema.get_fact_table("tf_nonexistent").is_none());
    }
}
