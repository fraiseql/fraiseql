//! UNION ALL SQL builder for partial-period aggregation.
//!
//! Generates a parameterized UNION ALL query combining up to 3 branches:
//! - **Branch 1** (partial leading): fine-grain rows with `DATE_TRUNC` re-aggregation
//! - **Branch 2** (complete middle): coarse-grain rows (already period-aligned)
//! - **Branch 3** (current period): fine-grain rows with `DATE_TRUNC` re-aggregation
//!
//! All date range conditions are parameterized (`$N`). Extra WHERE conditions (non-date
//! filters like tenant isolation) are passed through to every branch.

use chrono::NaiveDate;

use super::{AggregationSqlGenerator, Result};
use crate::{
    compiler::{
        aggregation::{AggregateExpression, AggregationPlan, GroupByExpression},
        fact_table::{FactTableMetadata, PartialPeriodConfig, TemporalGrain},
    },
    db::where_clause::WhereClause,
    runtime::partial_period::BranchPlan,
};

/// Assembled UNION ALL query with bind parameters for partial-period aggregation.
#[derive(Debug, Clone)]
pub struct PartialPeriodSql {
    /// The complete UNION ALL SQL query.
    pub sql:    String,
    /// All bind parameters in placeholder order across all branches.
    pub params: Vec<serde_json::Value>,
}

impl AggregationSqlGenerator {
    /// Generates a UNION ALL query for partial-period aggregation.
    ///
    /// Combines fine-grain and coarse-grain branches according to the `BranchPlan`.
    /// Each branch gets the same SELECT columns and aggregate expressions, but
    /// fine-grain branches apply `DATE_TRUNC` to the time-grain column while coarse
    /// branches use it directly.
    ///
    /// # Errors
    ///
    /// Returns error if SQL expression generation fails.
    pub fn generate_partial_period(
        &self,
        plan: &AggregationPlan,
        config: &PartialPeriodConfig,
        branch_plan: &BranchPlan,
        extra_where: Option<&WhereClause>,
    ) -> Result<PartialPeriodSql> {
        let mut params: Vec<serde_json::Value> = Vec::new();
        let mut branches: Vec<String> = Vec::new();

        // B1: partial leading period (fine-grain)
        if let Some((gte, lt)) = branch_plan.partial_leading {
            let sql = self.build_branch(
                plan,
                &config.fine_grain_view,
                &config.time_grain_column,
                Some(config.time_grain_trunc),
                gte,
                lt,
                extra_where,
                &plan.metadata,
                &mut params,
            )?;
            branches.push(sql);
        }

        // B2: complete middle periods (coarse-grain, original view)
        if let Some((gte, lt)) = branch_plan.complete_middle {
            let sql = self.build_branch(
                plan,
                &plan.request.table_name,
                &config.time_grain_column,
                None, // no DATE_TRUNC — already period-aligned
                gte,
                lt,
                extra_where,
                &plan.metadata,
                &mut params,
            )?;
            branches.push(sql);
        }

        // B3: current in-progress period (fine-grain)
        let (gte, lt) = branch_plan.current_period;
        let sql = self.build_branch(
            plan,
            &config.fine_grain_view,
            &config.time_grain_column,
            Some(config.time_grain_trunc),
            gte,
            lt,
            extra_where,
            &plan.metadata,
            &mut params,
        )?;
        branches.push(sql);

        // Assemble UNION ALL
        let union_sql = branches.join("\nUNION ALL\n");

        Ok(PartialPeriodSql {
            sql:    union_sql,
            params,
        })
    }

    /// Builds a single branch of the UNION ALL query.
    ///
    /// # Arguments
    ///
    /// * `plan` — the original aggregation plan (for GROUP BY / aggregate expressions)
    /// * `view_name` — the view to query (fine-grain or coarse-grain)
    /// * `time_col` — the time-grain column name
    /// * `trunc_grain` — if `Some`, apply `DATE_TRUNC` to `time_col` in GROUP BY (fine-grain);
    ///   if `None`, use `time_col` directly (coarse-grain, already period-aligned)
    /// * `date_gte` — inclusive lower bound of the date range
    /// * `date_lt` — exclusive upper bound of the date range
    /// * `extra_where` — additional WHERE conditions (e.g., tenant filter)
    /// * `metadata` — fact table metadata for WHERE clause generation
    /// * `params` — shared parameter vec (appended to by this call)
    #[allow(clippy::too_many_arguments)] // Reason: branch building has inherent parameter complexity
    fn build_branch(
        &self,
        plan: &AggregationPlan,
        view_name: &str,
        time_col: &str,
        trunc_grain: Option<TemporalGrain>,
        date_gte: NaiveDate,
        date_lt: NaiveDate,
        extra_where: Option<&WhereClause>,
        metadata: &FactTableMetadata,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        // Build SELECT clause — may need to rewrite temporal bucket expressions
        let select_sql = self.build_branch_select(
            &plan.group_by_expressions,
            &plan.aggregate_expressions,
            time_col,
            trunc_grain,
        )?;

        let from_sql = format!("FROM {view_name}");

        // Build WHERE: date range + extra conditions
        let where_sql =
            self.build_branch_where(time_col, date_gte, date_lt, extra_where, metadata, params)?;

        // Build GROUP BY — may need rewritten temporal expressions
        let group_sql = if !plan.group_by_expressions.is_empty() {
            self.build_branch_group_by(&plan.group_by_expressions, time_col, trunc_grain)?
        } else {
            String::new()
        };

        let mut parts: Vec<&str> = vec![&select_sql, &from_sql, &where_sql, &group_sql];
        parts.retain(|s| !s.is_empty());

        Ok(parts.join("\n"))
    }

    /// Builds the SELECT clause for a branch, replacing the temporal bucket expression
    /// for the time-grain column with `DATE_TRUNC` when `trunc_grain` is `Some`.
    fn build_branch_select(
        &self,
        group_by_expressions: &[GroupByExpression],
        aggregate_expressions: &[AggregateExpression],
        time_col: &str,
        trunc_grain: Option<TemporalGrain>,
    ) -> Result<String> {
        let mut columns = Vec::new();

        for expr in group_by_expressions {
            let alias = group_by_alias(expr);
            let sql = self.branch_group_by_expr_sql(expr, time_col, trunc_grain)?;
            columns.push(format!("{sql} AS {alias}"));
        }

        for expr in aggregate_expressions {
            let column = self.aggregate_expression_to_sql(expr)?;
            let alias = aggregate_alias(expr);
            columns.push(format!("{column} AS {alias}"));
        }

        Ok(format!("SELECT\n  {}", columns.join(",\n  ")))
    }

    /// Builds the WHERE clause for a branch: date range + extra conditions.
    fn build_branch_where(
        &self,
        time_col: &str,
        date_gte: NaiveDate,
        date_lt: NaiveDate,
        extra_where: Option<&WhereClause>,
        metadata: &FactTableMetadata,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        let gte_param = self.emit_value_param(
            &serde_json::Value::String(date_gte.to_string()),
            params,
        );
        let lt_param = self.emit_value_param(
            &serde_json::Value::String(date_lt.to_string()),
            params,
        );

        let quoted_col = self.quote_identifier(time_col);
        let mut where_parts = vec![format!("{quoted_col} >= {gte_param} AND {quoted_col} < {lt_param}")];

        if let Some(wc) = extra_where {
            let extra_sql = self.where_clause_to_sql_parameterized(wc, metadata, params)?;
            if !extra_sql.is_empty() {
                where_parts.push(extra_sql);
            }
        }

        Ok(format!("WHERE {}", where_parts.join(" AND ")))
    }

    /// Builds GROUP BY clause for a branch, with possible temporal rewrite.
    fn build_branch_group_by(
        &self,
        group_by_expressions: &[GroupByExpression],
        time_col: &str,
        trunc_grain: Option<TemporalGrain>,
    ) -> Result<String> {
        let mut exprs = Vec::new();
        for expr in group_by_expressions {
            exprs.push(self.branch_group_by_expr_sql(expr, time_col, trunc_grain)?);
        }
        Ok(format!("GROUP BY {}", exprs.join(", ")))
    }

    /// Converts a single GROUP BY expression to SQL for a branch.
    ///
    /// For fine-grain branches (`trunc_grain` is `Some`), temporal bucket expressions
    /// on the time-grain column are rewritten to use `DATE_TRUNC` at the coarse grain.
    /// For coarse branches (`trunc_grain` is `None`), the original expression is used.
    fn branch_group_by_expr_sql(
        &self,
        expr: &GroupByExpression,
        time_col: &str,
        trunc_grain: Option<TemporalGrain>,
    ) -> Result<String> {
        match (expr, trunc_grain) {
            // Fine-grain branch: rewrite TemporalBucket on the time-grain column
            // to use DATE_TRUNC at the coarse grain level.
            (
                GroupByExpression::TemporalBucket { column, .. },
                Some(grain),
            ) if column == time_col => {
                Ok(self.temporal_bucket_sql(column, grain.to_temporal_bucket()))
            }

            // Fine-grain branch: rewrite NativeColumn on the time-grain column
            (
                GroupByExpression::NativeColumn { column, .. },
                Some(grain),
            ) if column == time_col => {
                Ok(self.temporal_bucket_sql(column, grain.to_temporal_bucket()))
            }

            // Coarse branch: temporal bucket on the time-grain column uses
            // the column directly — data is already period-aligned.
            (
                GroupByExpression::TemporalBucket { column, .. }
                | GroupByExpression::NativeColumn { column, .. },
                None,
            ) if column == time_col => Ok(self.quote_identifier(column)),

            // All other expressions: use standard conversion
            _ => self.group_by_expression_to_sql(expr),
        }
    }
}

/// Extracts the alias from a `GroupByExpression`.
fn group_by_alias(expr: &GroupByExpression) -> &str {
    match expr {
        GroupByExpression::JsonbPath { alias, .. }
        | GroupByExpression::TemporalBucket { alias, .. }
        | GroupByExpression::CalendarPath { alias, .. }
        | GroupByExpression::NativeColumn { alias, .. } => alias,
    }
}

/// Extracts the alias from an `AggregateExpression`.
fn aggregate_alias(expr: &AggregateExpression) -> &str {
    match expr {
        AggregateExpression::Count { alias }
        | AggregateExpression::CountDistinct { alias, .. }
        | AggregateExpression::MeasureAggregate { alias, .. }
        | AggregateExpression::AdvancedAggregate { alias, .. }
        | AggregateExpression::BoolAggregate { alias, .. } => alias,
    }
}
