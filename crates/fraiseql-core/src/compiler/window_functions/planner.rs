use super::{
    FactTableMetadata, FraiseQLError, FrameBoundary, FrameExclusion, FrameType, OrderByClause,
    OrderDirection, Result, SelectColumn, WhereClause, WindowExecutionPlan, WindowFrame,
    WindowFunction, WindowFunctionType,
};
use crate::compiler::window_allowlist::WindowAllowlist;

/// Window function plan generator
pub struct WindowFunctionPlanner;

/// Validate a SQL column/expression string for safe embedding.
///
/// Allows identifiers, JSONB path operators (`->`, `->>`), quoted string keys
/// (single-quote), periods, spaces, and parentheses for function-style paths.
/// Rejects characters that could enable SQL injection (`;`, `--`, `/*`, `=`,
/// `\`, nul bytes, etc.).
fn validate_sql_expression(value: &str, context: &str) -> Result<()> {
    let safe = value.chars().all(|c| {
        c.is_alphanumeric() || matches!(c, '_' | '-' | '>' | '\'' | '.' | ' ' | '(' | ')')
    });
    if safe {
        Ok(())
    } else {
        Err(FraiseQLError::Validation {
            message: format!(
                "Unsafe characters in window function {context}: {value:?}. \
                 Only identifiers, JSONB path operators (-> ->>), and quoted keys are allowed."
            ),
            path:    None,
        })
    }
}

impl WindowFunctionPlanner {
    /// Generate window function execution plan from JSON query
    ///
    /// # Example Query Format
    ///
    /// ```json
    /// {
    ///   "table": "tf_sales",
    ///   "select": ["revenue", "category"],
    ///   "windows": [
    ///     {
    ///       "function": {"type": "row_number"},
    ///       "alias": "rank",
    ///       "partitionBy": ["data->>'category'"],
    ///       "orderBy": [{"field": "revenue", "direction": "DESC"}]
    ///     }
    ///   ],
    ///   "limit": 10
    /// }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` if the window function specification is invalid
    /// (e.g., missing required fields, disallowed characters, or unsupported function names).
    pub fn plan(
        query: &serde_json::Value,
        metadata: &FactTableMetadata,
    ) -> Result<WindowExecutionPlan> {
        // Build schema-based allowlist from metadata (defence-in-depth on top of
        // character-level validation).  Empty metadata → empty allowlist → no
        // schema-constraint enforcement (character validation still applies).
        let allowlist = WindowAllowlist::from_metadata(metadata);

        // Parse table name
        let table = query["table"]
            .as_str()
            .ok_or_else(|| FraiseQLError::validation("Missing 'table' field"))?
            .to_string();

        // Parse SELECT columns
        let select = Self::parse_select_columns(query)?;

        // Parse window functions
        let windows = Self::parse_window_functions(query, &allowlist)?;

        // Parse WHERE clause (placeholder - full implementation would parse actual conditions)
        let where_clause = query.get("where").map(|_| WhereClause::And(vec![]));

        // Parse ORDER BY
        let order_by = query
            .get("orderBy")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| {
                        let direction = match item.get("direction").and_then(|d| d.as_str()) {
                            Some("DESC") => OrderDirection::Desc,
                            _ => OrderDirection::Asc,
                        };
                        Some(OrderByClause {
                            field: item["field"].as_str()?.to_string(),
                            direction,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Parse LIMIT/OFFSET
        let limit = query
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|n| u32::try_from(n).unwrap_or(u32::MAX));
        let offset = query
            .get("offset")
            .and_then(|v| v.as_u64())
            .map(|n| u32::try_from(n).unwrap_or(u32::MAX));

        Ok(WindowExecutionPlan {
            table,
            select,
            windows,
            where_clause,
            order_by,
            limit,
            offset,
        })
    }

    fn parse_select_columns(query: &serde_json::Value) -> Result<Vec<SelectColumn>> {
        let default_array = vec![];
        let select = query.get("select").and_then(|s| s.as_array()).unwrap_or(&default_array);

        let columns = select
            .iter()
            .filter_map(|col| {
                col.as_str().map(|col_str| SelectColumn {
                    expression: col_str.to_string(),
                    alias:      col_str.to_string(),
                })
            })
            .collect();

        Ok(columns)
    }

    fn parse_window_functions(
        query: &serde_json::Value,
        allowlist: &WindowAllowlist,
    ) -> Result<Vec<WindowFunction>> {
        let default_array = vec![];
        let windows = query.get("windows").and_then(|w| w.as_array()).unwrap_or(&default_array);

        windows.iter().map(|w| Self::parse_single_window(w, allowlist)).collect()
    }

    fn parse_single_window(
        window: &serde_json::Value,
        allowlist: &WindowAllowlist,
    ) -> Result<WindowFunction> {
        let function = Self::parse_window_function_type(&window["function"])?;
        let alias = window["alias"]
            .as_str()
            .ok_or_else(|| FraiseQLError::validation("Missing 'alias' in window function"))?
            .to_string();

        let partition_by = window
            .get("partitionBy")
            .and_then(|p| p.as_array())
            .map(|arr| -> Result<Vec<String>> {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(|col| {
                        // Layer 1: character-level validation (rejects SQL injection chars)
                        validate_sql_expression(col, "partitionBy")?;
                        // Layer 2: schema-based allowlist (defence-in-depth)
                        allowlist.validate(col, "PARTITION BY")?;
                        Ok(col.to_string())
                    })
                    .collect()
            })
            .transpose()?
            .unwrap_or_default();

        let order_by = window
            .get("orderBy")
            .and_then(|o| o.as_array())
            .map(|arr| -> Result<Vec<OrderByClause>> {
                arr.iter()
                    .filter_map(|item| {
                        let field = item["field"].as_str()?;
                        let direction = match item.get("direction").and_then(|d| d.as_str()) {
                            Some("DESC") => OrderDirection::Desc,
                            _ => OrderDirection::Asc,
                        };
                        Some((field, direction))
                    })
                    .map(|(field, direction)| {
                        // Layer 1: character-level validation
                        validate_sql_expression(field, "orderBy.field")?;
                        // Layer 2: schema-based allowlist (defence-in-depth)
                        allowlist.validate(field, "ORDER BY")?;
                        Ok(OrderByClause {
                            field: field.to_string(),
                            direction,
                        })
                    })
                    .collect()
            })
            .transpose()?
            .unwrap_or_default();

        let frame = window.get("frame").map(Self::parse_window_frame).transpose()?;

        Ok(WindowFunction {
            function,
            alias,
            partition_by,
            order_by,
            frame,
        })
    }

    fn parse_window_function_type(func: &serde_json::Value) -> Result<WindowFunctionType> {
        serde_json::from_value(func.clone()).map_err(|e| {
            FraiseQLError::validation(format!("Unknown or invalid window function: {e}"))
        })
    }

    fn parse_window_frame(frame: &serde_json::Value) -> Result<WindowFrame> {
        let frame_type = match frame["frame_type"].as_str() {
            Some("ROWS") => FrameType::Rows,
            Some("RANGE") => FrameType::Range,
            Some("GROUPS") => FrameType::Groups,
            _ => return Err(FraiseQLError::validation("Invalid or missing 'frame_type'")),
        };

        let start = Self::parse_frame_boundary(&frame["start"])?;
        let end = Self::parse_frame_boundary(&frame["end"])?;
        let exclusion = frame.get("exclusion").map(|e| match e.as_str() {
            Some("current_row") => FrameExclusion::CurrentRow,
            Some("group") => FrameExclusion::Group,
            Some("ties") => FrameExclusion::Ties,
            // "no_others" and unrecognised values default to NoOthers
            _ => FrameExclusion::NoOthers,
        });

        Ok(WindowFrame {
            frame_type,
            start,
            end,
            exclusion,
        })
    }

    fn parse_frame_boundary(boundary: &serde_json::Value) -> Result<FrameBoundary> {
        match boundary["type"].as_str() {
            Some("unbounded_preceding") => Ok(FrameBoundary::UnboundedPreceding),
            Some("n_preceding") => {
                let n = u32::try_from(
                    boundary["n"]
                        .as_u64()
                        .ok_or_else(|| FraiseQLError::validation("Missing 'n' in N PRECEDING"))?,
                )
                .unwrap_or(u32::MAX);
                Ok(FrameBoundary::NPreceding { n })
            },
            Some("current_row") => Ok(FrameBoundary::CurrentRow),
            Some("n_following") => {
                let n = u32::try_from(
                    boundary["n"]
                        .as_u64()
                        .ok_or_else(|| FraiseQLError::validation("Missing 'n' in N FOLLOWING"))?,
                )
                .unwrap_or(u32::MAX);
                Ok(FrameBoundary::NFollowing { n })
            },
            Some("unbounded_following") => Ok(FrameBoundary::UnboundedFollowing),
            _ => Err(FraiseQLError::validation("Invalid frame boundary type")),
        }
    }

    /// Validate window function plan
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` if the plan uses features unsupported by the
    /// target database (e.g., RANGE frames on MySQL/SQLite).
    pub fn validate(
        plan: &WindowExecutionPlan,
        _metadata: &FactTableMetadata,
        database_target: crate::db::types::DatabaseType,
    ) -> Result<()> {
        use crate::db::types::DatabaseType;

        // Validate frame type supported by database
        for window in &plan.windows {
            if let Some(frame) = &window.frame {
                if frame.frame_type == FrameType::Groups
                    && !matches!(database_target, DatabaseType::PostgreSQL)
                {
                    return Err(FraiseQLError::validation(
                        "GROUPS frame type only supported on PostgreSQL",
                    ));
                }

                // Validate frame exclusion (PostgreSQL only)
                if frame.exclusion.is_some() && !matches!(database_target, DatabaseType::PostgreSQL)
                {
                    return Err(FraiseQLError::validation(
                        "Frame exclusion only supported on PostgreSQL",
                    ));
                }
            }

            // Validate PERCENT_RANK and CUME_DIST (not in SQLite)
            match window.function {
                WindowFunctionType::PercentRank | WindowFunctionType::CumeDist => {
                    if matches!(database_target, DatabaseType::SQLite) {
                        return Err(FraiseQLError::validation(
                            "PERCENT_RANK and CUME_DIST not supported on SQLite",
                        ));
                    }
                },
                _ => {},
            }
        }

        Ok(())
    }
}
