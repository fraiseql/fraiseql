use super::{
    FactTableMetadata, FraiseQLError, OrderByClause, PartitionByColumn, Result, SelectColumn,
    WindowExecutionPlan, WindowFunction, WindowFunctionRequest, WindowFunctionSpec,
    WindowFunctionType, WindowOrderBy, WindowRequest, WindowSelectColumn,
};

// =============================================================================
// WindowPlanner - Converts high-level WindowRequest to WindowExecutionPlan
// =============================================================================

/// High-level window planner that validates semantic names against metadata.
///
/// Converts `WindowRequest` (user-friendly semantic names) to `WindowExecutionPlan`
/// (SQL expressions ready for execution).
///
/// # Example
///
/// ```rust,ignore
/// let request = WindowRequest { ... };
/// let metadata = FactTableMetadata { ... };
/// let plan = WindowPlanner::plan(request, metadata)?;
/// // plan now has SQL expressions like "dimensions->>'category'" instead of "category"
/// ```
pub struct WindowPlanner;

impl WindowPlanner {
    /// Convert high-level WindowRequest to executable WindowExecutionPlan.
    ///
    /// # Arguments
    ///
    /// * `request` - High-level window request with semantic names
    /// * `metadata` - Fact table metadata for validation and expression generation
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Referenced measures don't exist in metadata
    /// - Referenced filter columns don't exist
    /// - Window function field references are invalid
    pub fn plan(
        request: WindowRequest,
        metadata: FactTableMetadata,
    ) -> Result<WindowExecutionPlan> {
        // Convert select columns to SQL expressions
        let select = Self::convert_select_columns(&request.select, &metadata)?;

        // Convert window functions to SQL expressions
        let windows = Self::convert_window_functions(&request.windows, &metadata)?;

        // Convert final ORDER BY to SQL expressions
        let order_by = Self::convert_order_by(&request.order_by, &metadata)?;

        Ok(WindowExecutionPlan {
            table: request.table_name,
            select,
            windows,
            where_clause: request.where_clause,
            order_by,
            limit: request.limit,
            offset: request.offset,
        })
    }

    /// Convert semantic select columns to SQL expressions.
    fn convert_select_columns(
        columns: &[WindowSelectColumn],
        metadata: &FactTableMetadata,
    ) -> Result<Vec<SelectColumn>> {
        columns
            .iter()
            .map(|col| Self::convert_single_select_column(col, metadata))
            .collect()
    }

    fn convert_single_select_column(
        column: &WindowSelectColumn,
        metadata: &FactTableMetadata,
    ) -> Result<SelectColumn> {
        match column {
            WindowSelectColumn::Measure { name, alias } => {
                // Validate measure exists
                if !metadata.measures.iter().any(|m| m.name == *name) {
                    return Err(FraiseQLError::Validation {
                        message: format!(
                            "Measure '{}' not found in fact table '{}'",
                            name, metadata.table_name
                        ),
                        path:    None,
                    });
                }
                // Measure columns are direct SQL columns
                Ok(SelectColumn {
                    expression: name.clone(),
                    alias:      alias.clone(),
                })
            },
            WindowSelectColumn::Dimension { path, alias } => {
                // Dimension from JSONB - generate extraction expression
                let expression = format!("{}->>'{}'", metadata.dimensions.name, path);
                Ok(SelectColumn {
                    expression,
                    alias: alias.clone(),
                })
            },
            WindowSelectColumn::Filter { name, alias } => {
                // Validate filter column exists
                if !metadata.denormalized_filters.iter().any(|f| f.name == *name) {
                    return Err(FraiseQLError::Validation {
                        message: format!(
                            "Filter column '{}' not found in fact table '{}'",
                            name, metadata.table_name
                        ),
                        path:    None,
                    });
                }
                // Filter columns are direct SQL columns
                Ok(SelectColumn {
                    expression: name.clone(),
                    alias:      alias.clone(),
                })
            },
        }
    }

    /// Convert semantic window functions to SQL expressions.
    fn convert_window_functions(
        windows: &[WindowFunctionRequest],
        metadata: &FactTableMetadata,
    ) -> Result<Vec<WindowFunction>> {
        windows
            .iter()
            .map(|w| Self::convert_single_window_function(w, metadata))
            .collect()
    }

    fn convert_single_window_function(
        request: &WindowFunctionRequest,
        metadata: &FactTableMetadata,
    ) -> Result<WindowFunction> {
        // Convert function spec to function type
        let function = Self::convert_function_spec(&request.function, metadata)?;

        // Convert PARTITION BY columns to SQL expressions
        let partition_by = request
            .partition_by
            .iter()
            .map(|p| Self::convert_partition_by(p, metadata))
            .collect::<Result<Vec<_>>>()?;

        // Convert ORDER BY within window to SQL expressions
        let order_by = request
            .order_by
            .iter()
            .map(|o| Self::convert_window_order_by(o, metadata))
            .collect::<Result<Vec<_>>>()?;

        Ok(WindowFunction {
            function,
            alias: request.alias.clone(),
            partition_by,
            order_by,
            frame: request.frame.clone(),
        })
    }

    /// Convert high-level function spec to low-level function type with SQL expressions.
    fn convert_function_spec(
        spec: &WindowFunctionSpec,
        metadata: &FactTableMetadata,
    ) -> Result<WindowFunctionType> {
        match spec {
            // Ranking functions - no field conversion needed
            WindowFunctionSpec::RowNumber => Ok(WindowFunctionType::RowNumber),
            WindowFunctionSpec::Rank => Ok(WindowFunctionType::Rank),
            WindowFunctionSpec::DenseRank => Ok(WindowFunctionType::DenseRank),
            WindowFunctionSpec::Ntile { n } => Ok(WindowFunctionType::Ntile { n: *n }),
            WindowFunctionSpec::PercentRank => Ok(WindowFunctionType::PercentRank),
            WindowFunctionSpec::CumeDist => Ok(WindowFunctionType::CumeDist),

            // Value functions - need field conversion
            WindowFunctionSpec::Lag {
                field,
                offset,
                default,
            } => {
                let sql_field = Self::resolve_field_to_sql(field, metadata)?;
                Ok(WindowFunctionType::Lag {
                    field:   sql_field,
                    offset:  *offset,
                    default: default.clone(),
                })
            },
            WindowFunctionSpec::Lead {
                field,
                offset,
                default,
            } => {
                let sql_field = Self::resolve_field_to_sql(field, metadata)?;
                Ok(WindowFunctionType::Lead {
                    field:   sql_field,
                    offset:  *offset,
                    default: default.clone(),
                })
            },
            WindowFunctionSpec::FirstValue { field } => {
                let sql_field = Self::resolve_field_to_sql(field, metadata)?;
                Ok(WindowFunctionType::FirstValue { field: sql_field })
            },
            WindowFunctionSpec::LastValue { field } => {
                let sql_field = Self::resolve_field_to_sql(field, metadata)?;
                Ok(WindowFunctionType::LastValue { field: sql_field })
            },
            WindowFunctionSpec::NthValue { field, n } => {
                let sql_field = Self::resolve_field_to_sql(field, metadata)?;
                Ok(WindowFunctionType::NthValue {
                    field: sql_field,
                    n:     *n,
                })
            },

            // Aggregate as window functions - need measure conversion
            WindowFunctionSpec::RunningSum { measure } => {
                Self::validate_measure(measure, metadata)?;
                Ok(WindowFunctionType::Sum {
                    field: measure.clone(),
                })
            },
            WindowFunctionSpec::RunningAvg { measure } => {
                Self::validate_measure(measure, metadata)?;
                Ok(WindowFunctionType::Avg {
                    field: measure.clone(),
                })
            },
            WindowFunctionSpec::RunningCount => Ok(WindowFunctionType::Count { field: None }),
            WindowFunctionSpec::RunningCountField { field } => {
                let sql_field = Self::resolve_field_to_sql(field, metadata)?;
                Ok(WindowFunctionType::Count {
                    field: Some(sql_field),
                })
            },
            WindowFunctionSpec::RunningMin { measure } => {
                Self::validate_measure(measure, metadata)?;
                Ok(WindowFunctionType::Min {
                    field: measure.clone(),
                })
            },
            WindowFunctionSpec::RunningMax { measure } => {
                Self::validate_measure(measure, metadata)?;
                Ok(WindowFunctionType::Max {
                    field: measure.clone(),
                })
            },
            WindowFunctionSpec::RunningStddev { measure } => {
                Self::validate_measure(measure, metadata)?;
                Ok(WindowFunctionType::Stddev {
                    field: measure.clone(),
                })
            },
            WindowFunctionSpec::RunningVariance { measure } => {
                Self::validate_measure(measure, metadata)?;
                Ok(WindowFunctionType::Variance {
                    field: measure.clone(),
                })
            },
        }
    }

    /// Convert PARTITION BY column to SQL expression.
    fn convert_partition_by(
        partition: &PartitionByColumn,
        metadata: &FactTableMetadata,
    ) -> Result<String> {
        match partition {
            PartitionByColumn::Dimension { path } => {
                Ok(format!("{}->>'{}'", metadata.dimensions.name, path))
            },
            PartitionByColumn::Filter { name } => {
                if !metadata.denormalized_filters.iter().any(|f| f.name == *name) {
                    return Err(FraiseQLError::Validation {
                        message: format!(
                            "Filter column '{}' not found in fact table '{}'",
                            name, metadata.table_name
                        ),
                        path:    None,
                    });
                }
                Ok(name.clone())
            },
            PartitionByColumn::Measure { name } => {
                Self::validate_measure(name, metadata)?;
                Ok(name.clone())
            },
        }
    }

    /// Convert window ORDER BY to SQL expression.
    fn convert_window_order_by(
        order: &WindowOrderBy,
        metadata: &FactTableMetadata,
    ) -> Result<OrderByClause> {
        let field = Self::resolve_field_to_sql(&order.field, metadata)?;
        Ok(OrderByClause {
            field,
            direction: order.direction,
        })
    }

    /// Convert final ORDER BY to SQL expressions.
    fn convert_order_by(
        orders: &[WindowOrderBy],
        metadata: &FactTableMetadata,
    ) -> Result<Vec<OrderByClause>> {
        orders.iter().map(|o| Self::convert_window_order_by(o, metadata)).collect()
    }

    /// Resolve a semantic field name to its SQL expression.
    ///
    /// Priority:
    /// 1. Check if it's a measure (direct column)
    /// 2. Check if it's a filter column (direct column)
    /// 3. Treat as a dimension path (JSONB extraction) — only if the name is a valid GraphQL
    ///    identifier (`[_A-Za-z][_0-9A-Za-z]*`), to prevent SQL injection via the single-quoted key
    ///    in `data->>'field'` expressions.
    fn resolve_field_to_sql(field: &str, metadata: &FactTableMetadata) -> Result<String> {
        // Check if it's a measure
        if metadata.measures.iter().any(|m| m.name == field) {
            return Ok(field.to_string());
        }

        // Check if it's a filter column
        if metadata.denormalized_filters.iter().any(|f| f.name == field) {
            return Ok(field.to_string());
        }

        // Validate identifier before embedding in JSONB extraction expression.
        // Without this check, a field like "x'; DROP TABLE t; --" would produce
        // `data->>'x'; DROP TABLE t; --'`, breaking the SQL structure.
        Self::validate_field_identifier(field)?;

        // Dimension path
        Ok(format!("{}->>'{}'", metadata.dimensions.name, field))
    }

    /// Validate that `field` is a safe GraphQL identifier: `[_A-Za-z][_0-9A-Za-z]*`.
    ///
    /// Field names are embedded as single-quoted string keys in JSONB extraction
    /// expressions (`data->>'field'`). Any character outside this set must be rejected.
    fn validate_field_identifier(field: &str) -> Result<()> {
        let mut chars = field.chars();
        let first_ok = chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
        let rest_ok = chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
        if first_ok && rest_ok {
            Ok(())
        } else {
            Err(crate::error::FraiseQLError::Validation {
                message: format!(
                    "window field '{field}' contains invalid characters; \
                     only [_A-Za-z][_0-9A-Za-z]* is allowed"
                ),
                path:    None,
            })
        }
    }

    /// Validate that a measure exists in metadata.
    fn validate_measure(measure: &str, metadata: &FactTableMetadata) -> Result<()> {
        if !metadata.measures.iter().any(|m| m.name == *measure) {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "Measure '{}' not found in fact table '{}'",
                    measure, metadata.table_name
                ),
                path:    None,
            });
        }
        Ok(())
    }
}
