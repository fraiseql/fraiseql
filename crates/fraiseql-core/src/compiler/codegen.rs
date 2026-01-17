//! Code generator - produces final CompiledSchema JSON.
//!
//! # Overview
//!
//! Takes validated IR and SQL templates, produces runtime-optimized
//! CompiledSchema ready for execution.

use crate::error::Result;
use crate::schema::CompiledSchema;
use super::ir::AuthoringIR;
use super::lowering::SqlTemplate;

/// Code generator.
pub struct CodeGenerator {
    optimize: bool,
}

impl CodeGenerator {
    /// Create new code generator.
    #[must_use]
    pub fn new(optimize: bool) -> Self {
        Self { optimize }
    }

    /// Generate CompiledSchema from IR and SQL templates.
    ///
    /// # Arguments
    ///
    /// * `ir` - Validated IR
    /// * `templates` - SQL templates
    ///
    /// # Returns
    ///
    /// CompiledSchema ready for runtime execution
    ///
    /// # Errors
    ///
    /// Returns error if code generation fails.
    pub fn generate(&self, ir: &AuthoringIR, _templates: &[SqlTemplate]) -> Result<CompiledSchema> {
        // TODO: Transform IR + templates into CompiledSchema
        // For now, create empty schema with types/queries from IR

        use crate::schema::{TypeDefinition, QueryDefinition, MutationDefinition};
        use crate::schema::AutoParams as SchemaAutoParams;

        let types = ir.types.iter().map(|t| {
            TypeDefinition {
                name: t.name.clone(),
                sql_source: t.sql_source.clone().unwrap_or_else(|| t.name.clone()),
                jsonb_column: "data".to_string(),
                fields: Vec::new(), // TODO: Map fields
                description: t.description.clone(),
                sql_projection_hint: None, // TODO: Generate projection hints during optimization
                implements: Vec::new(), // TODO: Map implements from intermediate
            }
        }).collect();

        let queries = ir.queries.iter().map(|q| {
            QueryDefinition {
                name: q.name.clone(),
                return_type: q.return_type.clone(),
                returns_list: q.returns_list,
                nullable: q.nullable,
                arguments: Vec::new(), // TODO: Map arguments
                sql_source: q.sql_source.clone(),
                description: q.description.clone(),
                auto_params: SchemaAutoParams {
                    has_where: q.auto_params.has_where,
                    has_order_by: q.auto_params.has_order_by,
                    has_limit: q.auto_params.has_limit,
                    has_offset: q.auto_params.has_offset,
                },
                deprecation: None, // TODO: Map deprecation from intermediate
            }
        }).collect();

        let mutations = ir.mutations.iter().map(|m| {
            MutationDefinition {
                name: m.name.clone(),
                return_type: m.return_type.clone(),
                arguments: Vec::new(), // TODO: Map arguments
                description: m.description.clone(),
                operation: crate::schema::MutationOperation::default(),
                deprecation: None, // TODO: Map deprecation from intermediate
            }
        }).collect();

        Ok(CompiledSchema {
            types,
            enums: Vec::new(), // TODO: Map enums from intermediate
            input_types: Vec::new(), // TODO: Map input types from intermediate
            interfaces: Vec::new(), // TODO: Map interfaces from intermediate
            unions: Vec::new(), // TODO: Map unions from intermediate
            queries,
            mutations,
            subscriptions: Vec::new(), // TODO: Map subscriptions
            directives: Vec::new(), // TODO: Map custom directives from intermediate
            fact_tables: std::collections::HashMap::new(), // Will be populated by compiler
        })
    }

    /// Check if optimization is enabled.
    #[must_use]
    pub const fn optimize(&self) -> bool {
        self.optimize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code_generator_new() {
        let generator = CodeGenerator::new(true);
        assert!(generator.optimize());

        let generator = CodeGenerator::new(false);
        assert!(!generator.optimize());
    }

    #[test]
    fn test_generate_empty_schema() {
        let generator = CodeGenerator::new(true);
        let ir = AuthoringIR::new();
        let templates = Vec::new();

        let result = generator.generate(&ir, &templates);
        assert!(result.is_ok());

        let schema = result.unwrap();
        assert!(schema.types.is_empty());
        assert!(schema.queries.is_empty());
    }
}
