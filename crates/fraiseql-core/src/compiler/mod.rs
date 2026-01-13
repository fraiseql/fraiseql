//! Schema compiler for FraiseQL v2.
//!
//! # Overview
//!
//! The compiler transforms GraphQL schema definitions (from Python/TypeScript decorators)
//! into optimized, executable `CompiledSchema` with pre-generated SQL templates.
//!
//! # Compilation Pipeline
//!
//! ```text
//! JSON Schema (from decorators)
//!         ↓
//!    Parser (parse.rs)
//!         ↓
//! Authoring IR (ir.rs)
//!         ↓
//!   Validator (validator.rs)
//!         ↓
//!    Lowering (lowering.rs)
//!         ↓
//!   SQL Templates
//!         ↓
//!    Codegen (codegen.rs)
//!         ↓
//! CompiledSchema JSON
//! ```
//!
//! # Phases
//!
//! 1. **Parse**: JSON schema → Authoring IR
//! 2. **Validate**: Type checking, binding validation, auth rules
//! 3. **Lower**: IR → SQL templates (database-specific)
//! 4. **Codegen**: SQL templates → CompiledSchema JSON
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
pub mod fact_table;
pub mod ir;
mod lowering;
pub mod parser;
pub mod validator;
pub mod window_functions;

pub use aggregate_types::{AggregateTypeGenerator, AggregateType, GroupByInput, HavingInput};
pub use aggregation::{AggregationPlanner, AggregationPlan, AggregationRequest};
pub use codegen::CodeGenerator;
pub use ir::{AuthoringIR, IRType, IRField, IRQuery, IRMutation, IRSubscription, IRArgument, AutoParams, MutationOperation};
pub use lowering::{SqlTemplateGenerator, DatabaseTarget};
pub use parser::SchemaParser;
pub use validator::{SchemaValidator, ValidationError};
pub use window_functions::{WindowFunctionPlanner, WindowExecutionPlan, WindowFunction};

use crate::error::Result;
use crate::schema::CompiledSchema;

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
            optimize_sql: true,
            strict_mode: false,
            debug: false,
            database_url: None,
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
    config: CompilerConfig,
    parser: SchemaParser,
    validator: SchemaValidator,
    lowering: SqlTemplateGenerator,
    codegen: CodeGenerator,
}

impl Compiler {
    /// Create new compiler with default configuration.
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(CompilerConfig::default())
    }

    /// Create new compiler with custom configuration.
    #[must_use]
    pub fn with_config(config: CompilerConfig) -> Self {
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
    /// * `schema_json` - JSON schema from Python/TypeScript decorators
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
        // Phase 1: Parse JSON → Authoring IR
        if self.config.debug {
            eprintln!("[compiler] Phase 1: Parsing schema...");
        }
        let ir = self.parser.parse(schema_json)?;

        // Phase 2: Validate IR
        if self.config.debug {
            eprintln!("[compiler] Phase 2: Validating schema...");
        }
        let validated_ir = self.validator.validate(ir)?;

        // Phase 3: Lower IR → SQL templates
        if self.config.debug {
            eprintln!("[compiler] Phase 3: Generating SQL templates...");
        }
        let sql_templates = self.lowering.generate(&validated_ir)?;

        // Phase 4: Codegen SQL templates → CompiledSchema
        if self.config.debug {
            eprintln!("[compiler] Phase 4: Generating CompiledSchema...");
        }
        let compiled = self.codegen.generate(&validated_ir, &sql_templates)?;

        // Note: Fact table metadata will be added by external tools or
        // through explicit API calls (e.g., from Python decorators)

        if self.config.debug {
            eprintln!("[compiler] Compilation complete!");
        }

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
    use super::*;

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
            optimize_sql: false,
            strict_mode: true,
            debug: true,
            database_url: None,
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

        let result = compiler.compile(schema_json);
        assert!(result.is_ok());

        let compiled = result.unwrap();
        assert_eq!(compiled.fact_tables.len(), 0);
    }

    #[test]
    fn test_compiled_schema_fact_table_operations() {
        let mut schema = CompiledSchema::new();

        // Test adding fact table
        let metadata = serde_json::json!({
            "table_name": "tf_sales",
            "measures": [],
            "dimensions": {"name": "data"},
            "denormalized_filters": []
        });

        schema.add_fact_table("tf_sales".to_string(), metadata.clone());

        // Test has_fact_tables
        assert!(schema.has_fact_tables());

        // Test list_fact_tables
        let tables = schema.list_fact_tables();
        assert_eq!(tables.len(), 1);
        assert!(tables.contains(&"tf_sales"));

        // Test get_fact_table
        let retrieved = schema.get_fact_table("tf_sales");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), &metadata);

        // Test get non-existent table
        assert!(schema.get_fact_table("tf_nonexistent").is_none());
    }
}
