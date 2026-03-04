use std::collections::HashSet;

use anyhow::{Context, Result, bail};
use fraiseql_core::schema::{
    ArgumentDefinition, AutoParams, CursorType, InjectedParamSource, QueryDefinition,
};
use tracing::warn;

use super::SchemaConverter;
use crate::schema::intermediate::{
    IntermediateArgument, IntermediateAutoParams, IntermediateQuery, IntermediateQueryDefaults,
};

impl SchemaConverter {
    pub(super) fn parse_inject_source(raw: &str) -> Result<InjectedParamSource> {
        if let Some(claim) = raw.strip_prefix("jwt:") {
            if claim.is_empty() {
                bail!("inject source 'jwt:' requires a claim name (e.g. 'jwt:org_id')");
            }
            return Ok(InjectedParamSource::Jwt(claim.to_owned()));
        }
        bail!(
            "Unknown inject source prefix in {raw:?}. \
             Supported: 'jwt:<claim_name>' (e.g. 'jwt:org_id', 'jwt:sub')"
        )
    }

    /// Convert inject map from intermediate format (raw strings) to compiled format.
    pub(super) fn convert_inject_params(
        op_name: &str,
        arg_names: &HashSet<&str>,
        inject: indexmap::IndexMap<String, String>,
    ) -> Result<indexmap::IndexMap<String, InjectedParamSource>> {
        inject
            .into_iter()
            .map(|(name, source)| {
                if arg_names.contains(name.as_str()) {
                    bail!(
                        "Operation '{op_name}': inject param '{name}' conflicts with an explicit \
                         argument name. Rename either the inject param or the argument."
                    );
                }
                Ok((name, Self::parse_inject_source(&source)?))
            })
            .collect()
    }

    /// Convert `IntermediateQuery` to `QueryDefinition`
    pub(super) fn convert_query(
        intermediate: IntermediateQuery,
        defaults: &IntermediateQueryDefaults,
    ) -> Result<QueryDefinition> {
        // Validate relay constraints before conversion.
        if intermediate.relay {
            if !intermediate.returns_list {
                anyhow::bail!(
                    "Query '{}': relay=true requires returns_list=true; \
                     Relay connections only apply to list queries",
                    intermediate.name
                );
            }
            if intermediate.sql_source.is_none() {
                anyhow::bail!(
                    "Query '{}': relay=true requires sql_source to be set; \
                     the compiler needs the view name to derive the cursor column \
                     (pk_{{snake_case(return_type)}})",
                    intermediate.name
                );
            }
        }

        let arguments = intermediate
            .arguments
            .into_iter()
            .map(Self::convert_argument)
            .collect::<Result<Vec<_>>>()
            .context(format!("Failed to convert query '{}'", intermediate.name))?;

        let arg_names: HashSet<&str> = arguments.iter().map(|a| a.name.as_str()).collect();
        let inject_params = Self::convert_inject_params(
            &intermediate.name,
            &arg_names,
            intermediate.inject,
        )
        .context(format!("Failed to convert inject params for query '{}'", intermediate.name))?;

        // Determine auto_params using the priority chain:
        //   1. Relay:       always {where:T, order_by:T, limit:F, offset:F} (spec-mandated)
        //   2. Single-item: always all-false (no auto-params)
        //   3. List:        resolve per-query override on top of TOML defaults
        let auto_params = if intermediate.relay {
            AutoParams {
                has_where:    true,
                has_order_by: true,
                has_limit:    false,
                has_offset:   false,
            }
        } else if intermediate.returns_list {
            let resolved =
                Self::resolve_auto_params(intermediate.auto_params.as_ref(), defaults);
            Self::warn_auto_params(&intermediate.name, &resolved);
            resolved
        } else {
            AutoParams::default()
        };

        let deprecation = intermediate
            .deprecated
            .map(|d| fraiseql_core::schema::DeprecationInfo { reason: d.reason });

        // Derive the keyset pagination column from the return type name.
        // Convention: User → pk_user, BlogPost → pk_blog_post (snake_case).
        let relay_cursor_column = if intermediate.relay {
            Some(format!("pk_{}", fraiseql_core::utils::to_snake_case(&intermediate.return_type)))
        } else {
            None
        };

        // Validate additional_views entries as safe SQL identifiers.
        for view in &intermediate.additional_views {
            if !Self::is_safe_sql_identifier(view) {
                anyhow::bail!(
                    "Query '{}': additional_views entry {:?} is not a valid SQL identifier. \
                     Use only letters, digits, and underscores (must start with a letter or \
                     underscore).",
                    intermediate.name,
                    view
                );
            }
        }

        Ok(QueryDefinition {
            name: intermediate.name,
            return_type: intermediate.return_type,
            returns_list: intermediate.returns_list,
            nullable: intermediate.nullable,
            arguments,
            sql_source: intermediate.sql_source,
            description: intermediate.description,
            auto_params,
            deprecation,
            jsonb_column: intermediate.jsonb_column.unwrap_or_else(|| "data".to_string()),
            relay: intermediate.relay,
            relay_cursor_column,
            relay_cursor_type: match intermediate.relay_cursor_type.as_deref() {
                Some("uuid") => CursorType::Uuid,
                _ => CursorType::Int64,
            },
            inject_params,
            cache_ttl_seconds: intermediate.cache_ttl_seconds,
            additional_views: intermediate.additional_views,
            requires_role: intermediate.requires_role,
        })
    }

    /// Convert `IntermediateArgument` to `ArgumentDefinition`
    pub(super) fn convert_argument(
        intermediate: IntermediateArgument,
    ) -> Result<ArgumentDefinition> {
        let arg_type = Self::parse_field_type(&intermediate.arg_type)?;

        let deprecation = intermediate
            .deprecated
            .map(|d| fraiseql_core::schema::DeprecationInfo { reason: d.reason });

        Ok(ArgumentDefinition {
            name: intermediate.name,
            arg_type,
            nullable: intermediate.nullable,
            default_value: intermediate.default,
            description: None,
            deprecation,
        })
    }

    /// Resolve the final `AutoParams` for a list query using the priority chain:
    ///
    /// - `per_query`: flags explicitly set by the authoring-language decorator (`Some(v)`)
    ///   or absent (`None` → inherit from defaults)
    /// - `defaults`:  project-wide values from `[query_defaults]` in `fraiseql.toml`
    ///
    /// Relay queries and single-item queries are handled separately in `convert_query`
    /// and never reach this function.
    pub(super) fn resolve_auto_params(
        per_query: Option<&IntermediateAutoParams>,
        defaults: &IntermediateQueryDefaults,
    ) -> AutoParams {
        match per_query {
            None => AutoParams {
                has_where:    defaults.where_clause,
                has_order_by: defaults.order_by,
                has_limit:    defaults.limit,
                has_offset:   defaults.offset,
            },
            Some(p) => AutoParams {
                has_where:    p.where_clause.unwrap_or(defaults.where_clause),
                has_order_by: p.order_by.unwrap_or(defaults.order_by),
                has_limit:    p.limit.unwrap_or(defaults.limit),
                has_offset:   p.offset.unwrap_or(defaults.offset),
            },
        }
    }

    /// Emit compile-time warnings for problematic auto-param combinations.
    ///
    /// Called for non-relay list queries after resolving their final `AutoParams`.
    pub(super) fn warn_auto_params(name: &str, params: &AutoParams) {
        if !params.has_limit {
            warn!(
                query = name,
                "List query '{name}' has limit disabled and is not a Relay query. \
                 This query is unbounded and may scan the full table. \
                 Consider a SQL-level LIMIT in the view, or use relay=true."
            );
        }
        if params.has_limit && !params.has_order_by {
            warn!(
                query = name,
                "List query '{name}' paginates (limit=true) without ordering \
                 (order_by=false). Results may be non-deterministic across pages. \
                 Enable order_by or add ORDER BY in the SQL view."
            );
        }
    }
}
