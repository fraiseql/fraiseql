use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result, bail};
use fraiseql_core::schema::{
    ArgumentDefinition, AutoParams, CursorType, InjectedParamSource, PaginationOrder,
    QueryDefinition,
};
use tracing::warn;

use super::{DeclaredTypeNames, SchemaConverter};
use crate::{
    config::toml_schema::PaginationPosture,
    schema::intermediate::{
        IntermediateArgument, IntermediateAutoParams, IntermediateQuery, IntermediateQueryDefaults,
    },
};

impl SchemaConverter {
    /// Parse a raw inject-source string (e.g. `"jwt:org_id"`) into an
    /// [`InjectedParamSource`].
    ///
    /// # Errors
    ///
    /// Returns an error if the string uses an unsupported prefix, or if the
    /// `jwt:` prefix is present but the claim name is empty.
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
        declared: &DeclaredTypeNames,
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

        // Validate count-sibling constraints (#938). Refusing at compile time keeps
        // the failure where the author can see it: a `count = true` that produced
        // nothing would look exactly like a working schema until a client asked
        // for the field and got "not found in schema".
        if intermediate.count {
            if !intermediate.returns_list {
                anyhow::bail!(
                    "Query '{}': count=true requires returns_list=true; a single-item \
                     query has nothing to count",
                    intermediate.name
                );
            }
            if intermediate.sql_source.is_none() {
                anyhow::bail!(
                    "Query '{}': count=true requires sql_source to be set; the count \
                     is issued as SELECT COUNT(*) against that view",
                    intermediate.name
                );
            }
            if intermediate.relay {
                anyhow::bail!(
                    "Query '{}': count=true is redundant with relay=true — a Relay \
                     connection already exposes `totalCount`, over the same rows. \
                     Drop count=true, or drop relay=true if you need offset paging \
                     with a total",
                    intermediate.name
                );
            }
        }

        let arguments = intermediate
            .arguments
            .into_iter()
            .map(|a| Self::convert_argument(a, declared))
            .collect::<Result<Vec<_>>>()
            .context(format!("Failed to convert query '{}'", intermediate.name))?;

        let arg_names: HashSet<&str> = arguments.iter().map(|a| a.name.as_str()).collect();
        let inject_params =
            Self::convert_inject_params(&intermediate.name, &arg_names, intermediate.inject)
                .context(format!(
                    "Failed to convert inject params for query '{}'",
                    intermediate.name
                ))?;

        // Determine auto_params using the priority chain:
        //   1. Relay:       always {where:T, order_by:T, limit:F, offset:F} (spec-mandated)
        //   2. Single-item: always all-false (no auto-params)
        //   3. List:        resolve per-query override on top of TOML defaults
        let auto_params = if intermediate.relay {
            AutoParams::relay()
        } else if intermediate.returns_list {
            Self::resolve_auto_params(intermediate.auto_params.as_ref(), defaults)
        } else {
            AutoParams::default()
        };

        // The order this query's offset pages are cut in (#1303), resolved here
        // for the same reason `relay_cursor_column` is: the other pagination
        // family already declares its ordering key in the compiled schema. Before
        // the auto-param warnings, which are about what this answers.
        let pagination_order = Self::resolve_pagination_order(
            &intermediate.name,
            intermediate.pagination_order.as_deref(),
            &auto_params,
            intermediate.returns_list,
            intermediate.relay,
            defaults.pagination_order,
        )?;

        if intermediate.returns_list && !intermediate.relay {
            Self::warn_auto_params(&intermediate.name, &auto_params, pagination_order.as_ref());
        }

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

        // #846: the authored `rest` block reaches the compiled artifact here. Both this
        // site and its mutation counterpart used to write `None` unconditionally.
        let (rest_path, rest_method) =
            Self::convert_rest_annotation("Query", &intermediate.name, intermediate.rest)?;

        // A streaming export delivers a sequence of rows; a single-item query has no
        // sequence to deliver. Refused at compile time rather than at the first
        // request, which is the last point the authored intent is still visible —
        // the same rule `convert_rest_annotation` applies to an unknown HTTP verb.
        if intermediate.rest_stream && !intermediate.returns_list {
            anyhow::bail!(
                "Query '{}': rest_stream = true requires a list-returning query. The \
                 streaming representations (NDJSON, CSV, XLSX) deliver a sequence of \
                 rows, and this query returns one.",
                intermediate.name
            );
        }

        let requires_actor =
            super::parse_requires_actor("Query", &intermediate.name, &intermediate.requires_actor)?;

        Ok(QueryDefinition {
            name: intermediate.name,
            return_type: intermediate.return_type,
            returns_list: intermediate.returns_list,
            // The count sibling is derived from this definition by
            // `count_sibling()`, never authored directly.
            returns_count: false,
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
            pagination_order,
            inject_params,
            read_routing: intermediate.read_routing,
            cache_ttl_seconds: intermediate.cache_ttl_seconds,
            additional_views: intermediate.additional_views,
            requires_role: intermediate.requires_role,
            requires_actor,
            rest_path,
            rest_method,
            rest_stream: intermediate.rest_stream,
            native_columns: HashMap::new(),
        })
    }

    /// Convert `IntermediateArgument` to `ArgumentDefinition`
    pub(super) fn convert_argument(
        intermediate: IntermediateArgument,
        declared: &DeclaredTypeNames,
    ) -> Result<ArgumentDefinition> {
        let arg_type = Self::parse_field_type(&intermediate.arg_type, declared)?;

        let deprecation = intermediate
            .deprecated
            .map(|d| fraiseql_core::schema::DeprecationInfo { reason: d.reason });

        let default_value = intermediate
            .default
            .map(|v| fraiseql_core::schema::GraphQLValue::from_json(&v))
            .transpose()
            .with_context(|| {
                format!("invalid default value for argument `{}`", intermediate.name)
            })?;

        Ok(ArgumentDefinition {
            name: intermediate.name,
            arg_type,
            nullable: intermediate.nullable,
            default_value,
            description: intermediate.description,
            deprecation,
        })
    }

    /// Resolve the final `AutoParams` for a list query using the priority chain:
    ///
    /// - `per_query`: flags explicitly set by the authoring-language decorator (`Some(v)`) or
    ///   absent (`None` → inherit from defaults)
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

    /// Resolve the order this query's `LIMIT`/`OFFSET` pages are cut in (#1303).
    ///
    /// Offline — no database is required and none is consulted. A compile with
    /// `--database` sharpens the derived answer afterwards (`pk_<type>`, then a
    /// native `id` column, both cheaper than the JSONB extraction); a compile
    /// without one still produces a **correct** answer, because `compile` takes
    /// the database URL optionally and a derivation that needed introspection
    /// would be a derivation that breaks offline compiles.
    ///
    /// | authored | paginates | posture | result |
    /// |---|---|---|---|
    /// | `"none"` | yes | `identity`/`allow` | `None` — the author owns the order |
    /// | `"none"` | yes | `refuse` | a compile error |
    /// | a column | yes | any | [`PaginationOrder::Column`] |
    /// | absent | yes | `identity`/`refuse` | [`PaginationOrder::JsonIdentity`] |
    /// | absent | yes | `allow` | `None` — the pre-#1303 behaviour |
    /// | anything | no | any | `None`, or an error if the author declared one |
    ///
    /// # Errors
    ///
    /// Returns an error when the declared column is not a safe SQL identifier —
    /// it is interpolated into `ORDER BY` — when a query that cannot paginate
    /// declares one at all, or when a `"none"` opt-out meets the `refuse` posture.
    /// The second is refused rather than ignored because an ordering that silently
    /// applies to nothing looks exactly like one that works, and the author is the
    /// only one who can see the difference.
    pub(super) fn resolve_pagination_order(
        name: &str,
        authored: Option<&str>,
        auto_params: &AutoParams,
        returns_list: bool,
        relay: bool,
        posture: PaginationPosture,
    ) -> Result<Option<PaginationOrder>> {
        // Relay is keyset-paginated on `relay_cursor_column` and never reads
        // `limit`/`offset` (`AutoParams::relay` turns both off), so it is
        // excluded by the flags below as well — naming it here makes the
        // exclusion a statement rather than a consequence.
        let paginates = returns_list && !relay && (auto_params.has_limit || auto_params.has_offset);

        let Some(authored) = authored else {
            // `allow` derives nothing — a query is ordered only where its author
            // said so. `refuse` derives exactly what `identity` does; the two
            // differ only on whether the opt-out below is permitted.
            return Ok(match posture {
                PaginationPosture::Identity | PaginationPosture::Refuse => {
                    paginates.then_some(PaginationOrder::JsonIdentity)
                },
                PaginationPosture::Allow => None,
            });
        };

        if !paginates {
            bail!(
                "Query '{name}': pagination_order = {authored:?} is set on a query that does \
                 not paginate. It orders `LIMIT`/`OFFSET` pages, and this query has none — \
                 {}. Drop the setting, or enable limit/offset.",
                if relay {
                    "relay=true paginates by cursor, ordered by relay_cursor_column"
                } else if returns_list {
                    "auto_params disables both limit and offset"
                } else {
                    "it returns a single item"
                }
            );
        }

        if authored.eq_ignore_ascii_case("none") {
            if posture == PaginationPosture::Refuse {
                bail!(
                    "Query '{name}': pagination_order = \"none\" is refused — \
                     [query_defaults] pagination_order = \"refuse\" says every paginated read \
                     in this deployment has a total order, with no exceptions. Declare a \
                     unique column instead, or change the posture."
                );
            }
            return Ok(None);
        }

        if !fraiseql_core::schema::is_safe_sql_identifier(authored) {
            bail!(
                "Query '{name}': pagination_order {authored:?} is not a valid SQL identifier. \
                 Use only letters, digits and underscores — the value is interpolated into \
                 ORDER BY, not bound as a parameter."
            );
        }

        Ok(Some(PaginationOrder::Column(authored.to_owned())))
    }

    /// Emit compile-time warnings for problematic auto-param combinations.
    ///
    /// Called for non-relay list queries after resolving their final `AutoParams`
    /// and the order their pages are cut in — the second decides whether the
    /// first has anything to say about page determinism (#1303).
    pub(super) fn warn_auto_params(
        name: &str,
        params: &AutoParams,
        pagination_order: Option<&PaginationOrder>,
    ) {
        for message in Self::auto_param_warnings(name, params, pagination_order) {
            warn!(query = name, "{message}");
        }
    }

    /// What [`warn_auto_params`](Self::warn_auto_params) has to say about a resolved
    /// `AutoParams`, as values.
    ///
    /// Split from the emission so the rules can be asserted. A `tracing::warn!` is a
    /// side effect no test in this crate observes, and all three of these warnings are
    /// about a configuration that compiles cleanly and misbehaves at request time —
    /// exactly the kind that must not be able to go missing unnoticed.
    pub(super) fn auto_param_warnings(
        name: &str,
        params: &AutoParams,
        pagination_order: Option<&PaginationOrder>,
    ) -> Vec<String> {
        let mut warnings = Vec::new();
        if !params.has_limit {
            warnings.push(format!(
                "List query '{name}' has limit disabled and is not a Relay query. \
                 This query is unbounded and may scan the full table. \
                 Consider a SQL-level LIMIT in the view, or use relay=true."
            ));
        }
        // #1303: this used to fire on `limit && !order_by`, which was both too
        // narrow and too wide. Too narrow because a query the client *can* sort is
        // just as non-deterministic when the client does not; too wide because a
        // query whose pages carry a total order is deterministic whatever
        // `order_by` says. The condition is now the thing itself: this query
        // paginates and nothing orders its pages. It is reachable in exactly two
        // ways — the author declared `pagination_order = "none"`, or the
        // deployment set `[query_defaults] pagination_order = "allow"` — and in
        // both the answer is a SQL-level `ORDER BY`, which is the only remedy left.
        if params.has_limit && pagination_order.is_none() {
            warnings.push(format!(
                "List query '{name}' paginates and nothing orders its pages, so two pages of \
                 the same relation can overlap and skip rows. The SQL view must carry its own \
                 ORDER BY, or declare pagination_order to have the compiler apply one."
            ));
        }
        // #1283: the configuration is legal, and the REST surface it produces is a list
        // route that accepts no filter at all. Worth saying at compile time because the
        // author is the only one who can see the setting: a client discovers it as a
        // `400` on every filter parameter, and before the refusal existed it discovered
        // nothing — the filter was validated, dropped, and the whole relation returned
        // under a `200`.
        if !params.has_where {
            warnings.push(format!(
                "List query '{name}' accepts no client filter (where_clause = false). \
                 Over REST every filter parameter — `?field=value`, `?field[op]=value`, \
                 `?filter=`, `?or=`/`?and=`/`?not=` and `?search=` — is refused with 400, \
                 and the generated OpenAPI document publishes none of them; over GraphQL \
                 the query exposes no `where` argument. Set where_clause = true to make \
                 this query filterable."
            ));
        }
        warnings
    }
}
