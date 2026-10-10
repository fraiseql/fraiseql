//! SQL for an aggregate over measures reduced per entity and per bucket first (#1459).
//!
//! The reduction is a CTE whose rows are one (entity, bucket) each, carrying the grouping
//! outputs under their aliases and each reduced measure; the requested aggregate then runs
//! over it with the ordinary SELECT / GROUP BY / HAVING / ORDER BY builders, every grouping
//! read back as the CTE column of its alias.
//!
//! A carried-forward reduction (`last`, `first`) crosses the buckets of the requested range
//! with the entities and reads each cell's row by an index seek on `(entity…, over)`. The
//! seek applies the request's upper bounds on `over` but never its lower ones: the value
//! carried into the first bucket was recorded before it.

use std::fmt::Write as _;

use super::{
    AggregateExpression, AggregationSqlGenerator, GroupByExpression, ParameterizedAggregationSql,
    Result, TemporalBucket, ValidatedHavingCondition, expressions::group_by_alias,
};
use crate::{
    backend::where_clause::WhereOperator,
    compiler::{
        aggregation::AggregationPlan,
        time_reduction::{Reduction, TimeReduction, time_bounds},
    },
    error::FraiseQLError,
};

const REDUCED: &str = "\"__fraiseql_reduced\"";
const BUCKET: &str = "\"__fraiseql_bucket\"";
const PRIORITY: &str = "\"__fraiseql_priority\"";

/// The SQL that counts the cells a carried-forward reduction would read.
///
/// It counts the buckets and the entities, each no further than `bound + 1`, so a vast range
/// costs nothing to refuse. Neither count reads a measure.
#[derive(Debug, Clone)]
pub struct CellCountSql {
    /// One row: `buckets`, `entities`.
    pub sql:    String,
    /// Bind parameters in placeholder order.
    pub params: Vec<serde_json::Value>,
}

/// The step between two buckets of `bucket`.
const fn bucket_step(bucket: TemporalBucket) -> &'static str {
    match bucket {
        TemporalBucket::Second => "interval '1 second'",
        TemporalBucket::Minute => "interval '1 minute'",
        TemporalBucket::Hour => "interval '1 hour'",
        TemporalBucket::Day => "interval '1 day'",
        TemporalBucket::Week => "interval '1 week'",
        TemporalBucket::Month => "interval '1 month'",
        TemporalBucket::Quarter => "interval '3 months'",
        TemporalBucket::Year => "interval '1 year'",
    }
}

/// The filter fragments a carried-forward reduction composes.
struct CarryFilter {
    /// Every conjunct not on the time column (`TRUE` when none).
    rest:  String,
    /// The upper bounds on the time column (`TRUE` when none).
    upper: String,
    /// The earliest time in range, as an expression of the column's type.
    lo:    String,
    /// The latest time in range, as an expression of the column's type.
    hi:    String,
}

impl AggregationSqlGenerator {
    /// The SQL that reads a declared measure, as the plain aggregate does.
    fn measure_sql(&self, plan: &AggregationPlan, name: &str) -> String {
        plan.metadata
            .native_measures
            .get(name)
            .map_or_else(|| name.to_string(), |column| self.quote_identifier(column))
    }

    fn conjunction(parts: &[String]) -> String {
        if parts.is_empty() {
            "TRUE".to_string()
        } else {
            parts.join(" AND ")
        }
    }

    /// Split the request filter for a carried-forward reduction and render its fragments.
    fn carry_filter(
        &self,
        plan: &AggregationPlan,
        reduction: &TimeReduction,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<CarryFilter> {
        let refused = || {
            FraiseQLError::validation(format!(
                "`{}` can be filtered only by comparisons joined by and",
                reduction.over
            ))
        };
        let split = match &plan.request.where_clause {
            Some(clause) => time_bounds(clause, &reduction.over).ok_or_else(refused)?,
            None => crate::compiler::time_reduction::SplitFilter::default(),
        };
        let table = &plan.metadata.table_name;
        let over = self.quote_identifier(&reduction.over);
        let (cast, unit) = if reduction.over_date {
            ("date", "1")
        } else {
            ("timestamptz", "interval '1 microsecond'")
        };

        let rest = split
            .rest
            .iter()
            .map(|c| self.where_clause_to_sql_parameterized(c, &plan.metadata, params))
            .collect::<Result<Vec<_>>>()?;
        let rest = Self::conjunction(&rest);

        let mut upper = Vec::new();
        let mut lows = Vec::new();
        let mut highs = Vec::new();
        for bound in &split.bounds {
            let value = self.emit_cast_param(bound.value, cast, params);
            match bound.operator {
                WhereOperator::Eq => {
                    upper.push(format!("{over} <= {value}"));
                    lows.push(value.clone());
                    highs.push(value);
                },
                WhereOperator::Gte => lows.push(value),
                WhereOperator::Gt => lows.push(format!("({value} + {unit})")),
                WhereOperator::Lte => {
                    upper.push(format!("{over} <= {value}"));
                    highs.push(value);
                },
                WhereOperator::Lt => {
                    upper.push(format!("{over} < {value}"));
                    highs.push(format!("({value} - {unit})"));
                },
                _ => return Err(refused()),
            }
        }
        let upper = Self::conjunction(&upper);
        let extreme = |function: &str, values: Vec<String>, default: String| match values.len() {
            0 => default,
            1 => values.into_iter().next().unwrap_or_default(),
            _ => format!("{function}({})", values.join(", ")),
        };
        let lo = extreme(
            "GREATEST",
            lows,
            format!("(SELECT min({over}) FROM {table} WHERE {rest} AND {upper})"),
        );
        let hi = extreme("LEAST", highs, format!("(SELECT max({over}) FROM {table} WHERE {rest})"));
        Ok(CarryFilter {
            rest,
            upper,
            lo,
            hi,
        })
    }

    fn bucket_series(reduction: &TimeReduction, filter: &CarryFilter) -> String {
        let grain = reduction.bucket.postgres_arg();
        format!(
            "generate_series(DATE_TRUNC('{grain}', {lo}), DATE_TRUNC('{grain}', {hi}), {step})",
            lo = filter.lo,
            hi = filter.hi,
            step = bucket_step(reduction.bucket),
        )
    }

    fn entity_columns(&self, reduction: &TimeReduction) -> Vec<String> {
        reduction.entity.iter().map(|c| self.quote_identifier(c)).collect()
    }

    /// The SQL counting the buckets and entities a carried-forward reduction would cross,
    /// each capped at `bound + 1`; `None` for a plan that carries nothing forward.
    ///
    /// # Errors
    ///
    /// A filter this generator cannot render.
    pub fn generate_cell_count(
        &self,
        plan: &AggregationPlan,
        bound: u64,
    ) -> Result<Option<CellCountSql>> {
        let Some(reduction) =
            plan.time_reduction.as_ref().filter(|r| r.reduction.carries_forward())
        else {
            return Ok(None);
        };
        let mut params = Vec::new();
        let filter = self.carry_filter(plan, reduction, &mut params)?;
        let cap = bound.saturating_add(1);
        let sql = format!(
            "SELECT\n  (SELECT count(*) FROM (SELECT {series} LIMIT {cap}) b) AS buckets,\n  \
             (SELECT count(*) FROM (SELECT DISTINCT {entities} FROM {table} WHERE {rest} AND \
             {upper} LIMIT {cap}) e) AS entities",
            series = Self::bucket_series(reduction, &filter),
            entities = self.entity_columns(reduction).join(", "),
            table = plan.metadata.table_name,
            rest = filter.rest,
            upper = filter.upper,
        );
        Ok(Some(CellCountSql { sql, params }))
    }

    /// The groupings other than the time bucket, as `expr AS "alias"`.
    fn other_groupings(
        &self,
        plan: &AggregationPlan,
        reduction: &TimeReduction,
    ) -> Result<Vec<(String, String)>> {
        plan.group_by_expressions
            .iter()
            .filter(|e| group_by_alias(e) != reduction.bucket_alias)
            .map(|e| {
                Ok((self.group_by_expression_to_sql(e)?, self.quote_identifier(group_by_alias(e))))
            })
            .collect()
    }

    /// The reduction CTE's body.
    fn reduced_rows(
        &self,
        plan: &AggregationPlan,
        reduction: &TimeReduction,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        let table = &plan.metadata.table_name;
        let over = self.quote_identifier(&reduction.over);
        let bucket_alias = self.quote_identifier(&reduction.bucket_alias);
        let groupings = self.other_groupings(plan, reduction)?;
        let measures: Vec<String> =
            reduction.measures.iter().map(|m| self.measure_sql(plan, m)).collect();
        let entities = self.entity_columns(reduction);

        if reduction.reduction.carries_forward() {
            let filter = self.carry_filter(plan, reduction, params)?;
            let step = bucket_step(reduction.bucket);
            let mut columns: Vec<String> =
                groupings.iter().map(|(expr, alias)| format!("{expr} AS {alias}")).collect();
            columns.extend(
                measures.iter().enumerate().map(|(i, m)| format!("{m} AS \"__fraiseql_m{i}\"")),
            );
            let columns = columns.join(", ");
            let entity_match = Self::conjunction(
                &entities
                    .iter()
                    .enumerate()
                    .map(|(i, c)| format!("{c} = e.\"__fraiseql_e{i}\""))
                    .collect::<Vec<_>>(),
            );
            let common = format!("{entity_match} AND {} AND {}", filter.rest, filter.upper);
            let bucket = format!("b.{BUCKET}");
            let last_by = |end: &str| {
                format!(
                    "SELECT {columns} FROM {table} WHERE {common} AND {over} < {end} ORDER BY \
                     {over} DESC LIMIT 1"
                )
            };
            let lateral = match reduction.reduction {
                Reduction::Last => last_by(&format!("{bucket} + {step}")),
                // The bucket's first row; else the last row before it, carried in. The
                // priority orders the two: a (parallel) append may return either first.
                Reduction::First => format!(
                    "(SELECT {columns}, 0 AS {PRIORITY} FROM {table} WHERE {common} AND {over} \
                     >= {bucket} AND {over} < {bucket} + {step} ORDER BY {over} ASC LIMIT 1)\n    \
                     UNION ALL\n    (SELECT {columns}, 1 AS {PRIORITY} FROM {table} WHERE \
                     {common} AND {over} < {bucket} ORDER BY {over} DESC LIMIT 1)\n    ORDER BY \
                     {PRIORITY} LIMIT 1"
                ),
                Reduction::Avg | Reduction::Min | Reduction::Max | Reduction::Delta => {
                    return Err(FraiseQLError::validation("not a carried-forward reduction"));
                },
            };
            let entity_columns = entities
                .iter()
                .enumerate()
                .map(|(i, c)| format!("{c} AS \"__fraiseql_e{i}\""))
                .collect::<Vec<_>>()
                .join(", ");
            Ok(format!(
                "WITH \"__fraiseql_buckets\" AS (SELECT {series} AS {BUCKET}),\n\
                 \"__fraiseql_entities\" AS (SELECT DISTINCT {entity_columns} FROM {table} WHERE \
                 {rest} AND {upper}),\n\
                 {REDUCED} AS (SELECT {bucket} AS {bucket_alias}, s.* FROM \
                 \"__fraiseql_buckets\" b CROSS JOIN \"__fraiseql_entities\" e CROSS JOIN \
                 LATERAL (\n    {lateral}) s)",
                series = Self::bucket_series(reduction, &filter),
                rest = filter.rest,
                upper = filter.upper,
            ))
        } else {
            let where_sql = match &plan.request.where_clause {
                Some(clause) => {
                    self.build_where_clause_parameterized(clause, &plan.metadata, params)?
                },
                None => String::new(),
            };
            let bucket_expr = self.temporal_bucket_sql(&over, reduction.bucket);
            let mut columns = vec![format!("{bucket_expr} AS {bucket_alias}")];
            columns.extend(groupings.iter().map(|(expr, alias)| format!("{expr} AS {alias}")));
            for (i, m) in measures.iter().enumerate() {
                let reduced = match reduction.reduction {
                    Reduction::Avg => format!("AVG({m})"),
                    Reduction::Min => format!("MIN({m})"),
                    Reduction::Max => format!("MAX({m})"),
                    Reduction::Delta => format!(
                        "(ARRAY_AGG({m} ORDER BY {over} DESC))[1] - (ARRAY_AGG({m} ORDER BY \
                         {over} ASC))[1]"
                    ),
                    Reduction::Last | Reduction::First => {
                        return Err(FraiseQLError::validation("a carried-forward reduction"));
                    },
                };
                columns.push(format!("{reduced} AS \"__fraiseql_m{i}\""));
            }
            let mut group = entities;
            group.push(bucket_expr);
            group.extend(groupings.into_iter().map(|(expr, _)| expr));
            Ok(format!(
                "WITH {REDUCED} AS (SELECT {} FROM {table} {where_sql} GROUP BY {})",
                columns.join(", "),
                group.join(", ")
            ))
        }
    }

    /// The requested aggregate re-addressed to the reduction CTE: each grouping is its
    /// alias's column, each reduced measure its slot.
    fn over_reduced(
        reduction: &TimeReduction,
        expr: &AggregateExpression,
        measure: Option<&str>,
    ) -> Result<AggregateExpression> {
        let (
            AggregateExpression::MeasureAggregate {
                function, alias, ..
            },
            Some(measure),
        ) = (expr, measure)
        else {
            return Err(FraiseQLError::validation(
                "a reduced aggregate reads a measure through a plain aggregate",
            ));
        };
        let slot = reduction.measures.iter().position(|m| m == measure).ok_or_else(|| {
            FraiseQLError::validation(format!("`{measure}` is not a reduced measure"))
        })?;
        Ok(AggregateExpression::MeasureAggregate {
            column:   format!("__fraiseql_m{slot}"),
            function: *function,
            alias:    alias.clone(),
            native:   true,
        })
    }

    /// Generate the SQL for a plan with a [`TimeReduction`].
    pub(super) fn generate_time_reduced(
        &self,
        plan: &AggregationPlan,
        reduction: &TimeReduction,
    ) -> Result<ParameterizedAggregationSql> {
        use crate::compiler::aggregation::AggregateSelection;

        let mut params = Vec::new();
        let mut sql = self.reduced_rows(plan, reduction, &mut params)?;

        let groupings: Vec<GroupByExpression> = plan
            .group_by_expressions
            .iter()
            .map(|e| {
                let alias = group_by_alias(e).to_string();
                GroupByExpression::NativeColumn {
                    column: alias.clone(),
                    pg_cast: String::new(),
                    alias,
                }
            })
            .collect();
        let measure_of = |selection: &AggregateSelection| match selection {
            AggregateSelection::MeasureAggregate { measure, .. } => Some(measure.clone()),
            _ => None,
        };
        let aggregates = plan
            .aggregate_expressions
            .iter()
            .zip(&plan.request.aggregates)
            .map(|(expr, selection)| {
                Self::over_reduced(reduction, expr, measure_of(selection).as_deref())
            })
            .collect::<Result<Vec<_>>>()?;
        let having = plan
            .having_conditions
            .iter()
            .zip(&plan.request.having)
            .map(|(condition, requested)| {
                Ok(ValidatedHavingCondition {
                    aggregate: Self::over_reduced(
                        reduction,
                        &condition.aggregate,
                        measure_of(&requested.aggregate).as_deref(),
                    )?,
                    operator:  condition.operator,
                    value:     condition.value.clone(),
                })
            })
            .collect::<Result<Vec<_>>>()?;

        let _ = write!(
            sql,
            "\n{}\nFROM {REDUCED}\n{}",
            self.build_select_clause(&groupings, &aggregates)?,
            self.build_group_by_clause(&groupings)?
        );
        let having_sql = self.build_having_clause_parameterized(&having, &mut params)?;
        if !having_sql.is_empty() {
            let _ = write!(sql, "\n{having_sql}");
        }
        if !plan.request.order_by.is_empty() {
            let order = self.build_order_by_clause(
                &plan.request.order_by,
                &groupings,
                &std::collections::HashSet::new(),
            )?;
            let _ = write!(sql, "\n{order}");
        }
        if let Some(limit) = plan.request.limit {
            let _ = write!(sql, "\nLIMIT {limit}");
        }
        if let Some(offset) = plan.request.offset {
            let _ = write!(sql, "\nOFFSET {offset}");
        }
        Ok(ParameterizedAggregationSql { sql, params })
    }
}
