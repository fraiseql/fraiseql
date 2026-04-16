//! Query planning — `plan_query()` without executing against the database.

use super::{Executor, QueryType};
use crate::{
    db::traits::DatabaseAdapter,
    error::{FraiseQLError, Result},
    runtime::suggest_similar,
};

impl<A: DatabaseAdapter> Executor<A> {
    /// Generate an explain plan for a query without executing it.
    ///
    /// Returns the SQL that would be generated, parameters, cost estimate,
    /// and views that would be accessed.
    ///
    /// # Errors
    ///
    /// Returns error if the query cannot be parsed or matched against the schema.
    pub fn plan_query(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
    ) -> Result<super::super::ExplainPlan> {
        let query_type = self.classify_query(query)?;

        match query_type {
            QueryType::Regular => {
                let query_match = self.matcher.match_query(query, variables)?;
                let view = query_match
                    .query_def
                    .sql_source
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string());
                let plan = self.planner.plan(&query_match)?;
                Ok(super::super::ExplainPlan {
                    sql:            plan.sql,
                    parameters:     plan.parameters,
                    estimated_cost: plan.estimated_cost,
                    views_accessed: vec![view],
                    query_type:     "regular".to_string(),
                })
            },
            QueryType::Mutation { ref name, .. } => {
                let mutation_def =
                    self.schema.mutations.iter().find(|m| m.name == *name).ok_or_else(|| {
                        let display_names: Vec<String> = self
                            .schema
                            .mutations
                            .iter()
                            .map(|m| self.schema.display_name(&m.name))
                            .collect();
                        let candidate_refs: Vec<&str> =
                            display_names.iter().map(String::as_str).collect();
                        let suggestion = suggest_similar(name, &candidate_refs);
                        let message = match suggestion.as_slice() {
                            [s] => format!(
                                "Mutation '{name}' not found in schema. Did you mean '{s}'?"
                            ),
                            _ => format!("Mutation '{name}' not found in schema"),
                        };
                        FraiseQLError::Validation {
                            message,
                            path: None,
                        }
                    })?;
                let fn_name =
                    mutation_def.sql_source.clone().unwrap_or_else(|| format!("fn_{name}"));
                Ok(super::super::ExplainPlan {
                    sql:            format!("SELECT * FROM {fn_name}(...)"),
                    parameters:     Vec::new(),
                    estimated_cost: 100,
                    views_accessed: vec![fn_name],
                    query_type:     "mutation".to_string(),
                })
            },
            QueryType::Aggregate(ref name) => {
                let sql_source = self
                    .schema
                    .queries
                    .iter()
                    .find(|q| q.name == *name)
                    .and_then(|q| q.sql_source.clone())
                    .unwrap_or_else(|| "unknown".to_string());
                Ok(super::super::ExplainPlan {
                    sql:            format!("SELECT ... FROM {sql_source} -- aggregate"),
                    parameters:     Vec::new(),
                    estimated_cost: 200,
                    views_accessed: vec![sql_source],
                    query_type:     "aggregate".to_string(),
                })
            },
            QueryType::Window(ref name) => {
                let sql_source = self
                    .schema
                    .queries
                    .iter()
                    .find(|q| q.name == *name)
                    .and_then(|q| q.sql_source.clone())
                    .unwrap_or_else(|| "unknown".to_string());
                Ok(super::super::ExplainPlan {
                    sql:            format!("SELECT ... FROM {sql_source} -- window"),
                    parameters:     Vec::new(),
                    estimated_cost: 250,
                    views_accessed: vec![sql_source],
                    query_type:     "window".to_string(),
                })
            },
            QueryType::IntrospectionSchema | QueryType::IntrospectionType(_) => {
                Ok(super::super::ExplainPlan {
                    sql:            String::new(),
                    parameters:     Vec::new(),
                    estimated_cost: 0,
                    views_accessed: Vec::new(),
                    query_type:     "introspection".to_string(),
                })
            },
            QueryType::Federation(_) => Ok(super::super::ExplainPlan {
                sql:            String::new(),
                parameters:     Vec::new(),
                estimated_cost: 0,
                views_accessed: Vec::new(),
                query_type:     "federation".to_string(),
            }),
            QueryType::NodeQuery { .. } => Ok(super::super::ExplainPlan {
                sql:            String::new(),
                parameters:     Vec::new(),
                estimated_cost: 50,
                views_accessed: Vec::new(),
                query_type:     "node".to_string(),
            }),
        }
    }
}
