//! An aggregate over a rolled-up fact table linked to a finer one (`grain.edges_from`),
//! for a range the coarse table's rows would split.
//!
//! The whole coarse periods inside the range are read from the coarse table, the rest of
//! the range from the finer one, each table summed under the request's own filter and
//! grouping, and the two sums added per group:
//!
//! ```sql
//! SELECT "k", SUM("p0") AS "amount_sum"
//! FROM (
//!   SELECT <k> AS "k", SUM(amount) AS "p0" FROM tf_day WHERE <filter> AND (day < $1 OR day >= $2) GROUP BY <k>
//!   UNION ALL
//!   SELECT <k> AS "k", SUM(amount) AS "p0" FROM tf_month WHERE <filter> AND day >= $3 AND day < $4 GROUP BY <k>
//! ) AS cover
//! GROUP BY "k" HAVING … ORDER BY … LIMIT … OFFSET …
//! ```
//!
//! Only a sum of an additive measure recombines: a coarse row holds its period's total, so
//! any other aggregate over coarse rows and finer rows together is neither table's answer,
//! and is refused. A request the coarse table answers on its own is read as before, and
//! every refusal is the coarse table's: the finer table's parse decides nothing but the
//! alignment of the range's bounds.

use std::{collections::HashMap, fmt::Write as _};

use fraiseql_time::jiff::civil::Date;
use serde_json::{Value, json};

use super::{AggregationSqlGenerator, ParameterizedAggregationSql};
use crate::{
    backend::{WhereClause, where_clause::WhereOperator},
    compiler::{
        aggregate_types::AggregateFunction,
        aggregation::{
            AggregateSelection, AggregationPlan, AggregationPlanner, AggregationRequest,
        },
        fact_table::{
            Additivity, FactGrain, FactTableMetadata, date_text, edges_answerable, read_date,
        },
    },
    error::{FraiseQLError, Result},
    runtime::{AggregateQueryParser, aggregate_parser::where_condition},
    schema::CompiledSchema,
};

/// How a request over a linked table is read, when the coarse table cannot read it alone.
#[derive(Debug)]
pub enum Cover {
    /// No whole coarse period lies inside the range: the finer table reads all of it.
    Fine(Box<FactTableMetadata>),
    /// The whole coarse periods are read from the coarse table, the rest from the finer.
    Split {
        /// The finer table.
        fine:   Box<FactTableMetadata>,
        /// The rows the finer table reads: those outside the whole coarse periods.
        edges:  Box<WhereClause>,
        /// The rows the coarse table reads: its whole periods inside the range.
        middle: Box<WhereClause>,
    },
}

/// Parse an aggregate request over `metadata`, and say how it is read: `None` when the
/// table reads it alone, as every request was read before tables were linked.
///
/// # Errors
///
/// The coarse table's refusal, in its own words, for anything but a range bound the finer
/// table's rows answer; a range bound neither table's rows answer is refused as the coarse
/// table refuses it.
pub fn parse(
    schema: &CompiledSchema,
    query_json: &Value,
    metadata: &FactTableMetadata,
    native_columns: &HashMap<String, String>,
) -> Result<(AggregationRequest, Option<Cover>)> {
    let refused = match AggregateQueryParser::parse(query_json, metadata, native_columns) {
        Ok(request) => return Ok((request, None)),
        Err(refused) => refused,
    };
    match parse_linked(schema, query_json, metadata, native_columns)? {
        Some(parsed) => Ok(parsed),
        None => Err(refused),
    }
}

/// The request with the coarse table's own parse and the range's bounds checked against
/// the finer grain, or `None` when no finer table answers it (the coarse refusal stands).
fn parse_linked(
    schema: &CompiledSchema,
    query_json: &Value,
    coarse: &FactTableMetadata,
    native_columns: &HashMap<String, String>,
) -> Result<Option<(AggregationRequest, Option<Cover>)>> {
    let Some((grain, fine)) = linked(schema, coarse) else {
        return Ok(None);
    };
    let Some(filter) = query_json.get("where").and_then(Value::as_object) else {
        return Ok(None);
    };
    let bounds: Vec<(&String, &Value, WhereOperator)> = filter
        .iter()
        .filter_map(|(key, value)| Some((key, value, bound_operator(key, &grain.column)?)))
        .collect();
    // Everything but the bounds is the coarse table's to refuse.
    let mut rest = query_json.clone();
    if let Some(filter) = rest.get_mut("where").and_then(Value::as_object_mut) {
        for (key, ..) in &bounds {
            filter.remove(key.as_str());
        }
    }
    let mut request = AggregateQueryParser::parse(&rest, coarse, native_columns)?;
    let (mut from, mut until): (Option<Date>, Option<Date>) = (None, None);
    let mut conditions = Vec::with_capacity(bounds.len());
    for (key, value, op) in bounds {
        let Some(day) = read_date(value) else {
            return Ok(None);
        };
        // Read as the finer table reads it, which refuses a bound splitting its own rows.
        let Ok(condition) = where_condition(key, value, fine, native_columns) else {
            return Ok(None);
        };
        conditions.push(condition);
        // The range as `[from, until)`.
        match op {
            WhereOperator::Gte => from = from.max(Some(day)),
            WhereOperator::Gt => from = from.max(day.tomorrow().ok()),
            WhereOperator::Lt => until = Some(until.map_or(day, |u| u.min(day))),
            _ => {
                let Some(next) = day.tomorrow().ok() else {
                    return Ok(None);
                };
                until = Some(until.map_or(next, |u| u.min(next)));
            },
        }
    }
    request.where_clause = Some(match request.where_clause.take() {
        Some(WhereClause::And(mut rest)) => {
            rest.extend(conditions);
            WhereClause::And(rest)
        },
        Some(other) => WhereClause::And(std::iter::once(other).chain(conditions).collect()),
        None => WhereClause::And(conditions),
    });
    // The whole coarse periods: from the first period start at or after `from`, to the
    // last period start at or before `until`.
    let start = match from {
        Some(day) => match grain.bucket.ceil(day) {
            Some(start) => Some(start),
            None => return Ok(Some((request, Some(Cover::Fine(Box::new(fine.clone())))))),
        },
        None => None,
    };
    let end = until.map(|day| grain.bucket.start(day));
    if let (Some(start), Some(end)) = (start, end) {
        if start >= end {
            return Ok(Some((request, Some(Cover::Fine(Box::new(fine.clone()))))));
        }
    }
    let condition = |op: &str, day: Date| {
        where_condition(
            &format!("{}_{op}", grain.column),
            &json!(date_text(day)),
            fine,
            native_columns,
        )
    };
    let below = start.map(|day| condition("lt", day)).transpose()?;
    let above = end.map(|day| condition("gte", day)).transpose()?;
    let inside: Vec<WhereClause> = [
        start.map(|day| condition("gte", day)).transpose()?,
        end.map(|day| condition("lt", day)).transpose()?,
    ]
    .into_iter()
    .flatten()
    .collect();
    let edges = Box::new(WhereClause::Or(below.into_iter().chain(above).collect()));
    Ok(Some((
        request,
        Some(Cover::Split {
            fine: Box::new(fine.clone()),
            edges,
            middle: Box::new(WhereClause::And(inside)),
        }),
    )))
}

/// `coarse`'s grain and the finer table it links, when the pair answers a cover.
fn linked<'s>(
    schema: &'s CompiledSchema,
    coarse: &'s FactTableMetadata,
) -> Option<(&'s FactGrain, &'s FactTableMetadata)> {
    let grain = coarse.grain.as_ref()?;
    let fine = schema.get_fact_table(grain.edges_from.as_deref()?)?;
    edges_answerable(coarse, fine).then_some((grain, fine))
}

/// The operator of a range bound `<column>_gte|_gt|_lt|_lte` on `column`.
fn bound_operator(key: &str, column: &str) -> Option<WhereOperator> {
    match key.strip_prefix(column)?.strip_prefix('_')? {
        "gte" => Some(WhereOperator::Gte),
        "gt" => Some(WhereOperator::Gt),
        "lt" => Some(WhereOperator::Lt),
        "lte" => Some(WhereOperator::Lte),
        _ => None,
    }
}

impl Cover {
    /// The read of `plan`, planned against the coarse table, as this cover reads it: the
    /// plan the rows are answered by, and its SQL.
    ///
    /// # Errors
    ///
    /// [`FraiseQLError::Validation`] for an aggregate that does not recombine across the
    /// two tables, naming it and the coarse table.
    pub fn prepare(
        self,
        generator: &AggregationSqlGenerator,
        plan: AggregationPlan,
    ) -> Result<(AggregationPlan, ParameterizedAggregationSql)> {
        match self {
            Self::Fine(fine) => {
                let mut request = plan.request.clone();
                request.table_name.clone_from(&fine.table_name);
                let mut read = AggregationPlanner::plan(request, *fine)?;
                read.group_by_expressions = plan.group_by_expressions;
                let sql = generator.generate_parameterized(&read)?;
                Ok((read, sql))
            },
            Self::Split {
                fine,
                edges,
                middle,
            } => {
                let sql = split_sql(generator, &plan, &fine, *edges, *middle)?;
                Ok((plan, sql))
            },
        }
    }
}

/// The measure a recombinable aggregate sums, or the coarse table's refusal of it.
fn summed<'a>(
    plan: &AggregationPlan,
    selection: &'a AggregateSelection,
    argument: &str,
) -> Result<&'a str> {
    let coarse = &plan.metadata;
    let refusal = |why: String| {
        let grain = coarse.grain.as_ref().map_or_else(
            || ("period".to_string(), String::new()),
            |g| (g.bucket.name().replace('_', "-"), g.column.clone()),
        );
        FraiseQLError::Validation {
            message: format!(
                "fact table `{}` holds one row per {period} of `{column}`: {why}. Bound the \
                 range where a {period} starts and ends to read the {period}s alone",
                coarse.table_name,
                period = grain.0,
                column = grain.1,
            ),
            path:    Some(argument.to_string()),
        }
    };
    let period = coarse
        .grain
        .as_ref()
        .map_or_else(|| "period".to_string(), |g| g.bucket.name().replace('_', "-"));
    let alias = selection.alias();
    let AggregateSelection::MeasureAggregate {
        measure,
        function: AggregateFunction::Sum,
        ..
    } = selection
    else {
        let what = if matches!(selection, AggregateSelection::Count { .. }) {
            format!("`{alias}` counts rows, and a {period}'s row stands for many finer rows")
        } else {
            format!(
                "a {period}'s row holds its total, so `{alias}` over it and finer rows is neither table's answer"
            )
        };
        return Err(refusal(format!(
            "this range's whole {period}s are read from it and its edges from finer rows, and \
             {what}; only a sum recombines"
        )));
    };
    match coarse.measure(measure).map(|m| &m.additivity) {
        Some(Additivity::Additive) => Ok(measure),
        Some(Additivity::SemiAdditive { over, .. } | Additivity::Delta { over, .. }) => {
            Err(refusal(format!(
                "`{measure}` is semi-additive over `{over}`: a {period}'s row is not declared to \
                 hold the {period}'s closing or opening value, so it cannot stand for the \
                 {period} beside finer rows; only a sum of an additive measure recombines"
            )))
        },
        _ => Err(refusal(format!(
            "`{alias}` does not sum an additive measure; only a sum of an additive measure \
             recombines across this range's {period}s and its edges"
        ))),
    }
}

/// The two tables' sums, added per group, with the request's `having`, `orderBy`, `limit`
/// and `offset` applied to the added groups.
fn split_sql(
    generator: &AggregationSqlGenerator,
    plan: &AggregationPlan,
    fine: &FactTableMetadata,
    edges: WhereClause,
    middle: WhereClause,
) -> Result<ParameterizedAggregationSql> {
    let request = &plan.request;
    // One partial sum per measure the request sums (a `having` tests a selected one).
    let mut measures: Vec<&str> = Vec::new();
    for selection in &request.aggregates {
        measures.push(summed(plan, selection, "aggregates")?);
    }
    measures.sort_unstable();
    measures.dedup();
    let partial = |measure: &str| {
        let index = measures.iter().position(|m| *m == measure).unwrap_or_default();
        format!("p{index}")
    };
    // The added sum of `measure`.
    let total = |measure: &str| format!("SUM({})", generator.quote_identifier(&partial(measure)));

    let mut inner = request.clone();
    inner.aggregates = measures
        .iter()
        .map(|measure| AggregateSelection::MeasureAggregate {
            measure:  (*measure).to_string(),
            function: AggregateFunction::Sum,
            alias:    partial(measure),
        })
        .collect();
    inner.having = Vec::new();
    inner.order_by = Vec::new();
    inner.limit = None;
    inner.offset = None;

    let mut params = Vec::new();
    let mut branch = |table: &FactTableMetadata, rows: WhereClause| -> Result<String> {
        let mut request = inner.clone();
        request.table_name.clone_from(&table.table_name);
        request.where_clause = Some(match request.where_clause.take() {
            Some(filter) => WhereClause::And(vec![filter, rows]),
            None => rows,
        });
        let mut read = AggregationPlanner::plan(request, table.clone())?;
        read.group_by_expressions.clone_from(&plan.group_by_expressions);
        generator.generate_into(&read, &mut params)
    };
    let fine_sql = branch(fine, edges)?;
    let coarse_sql = branch(&plan.metadata, middle)?;

    let keys: Vec<String> = plan
        .group_by_expressions
        .iter()
        .map(|expr| generator.quote_identifier(super::expressions::group_by_alias(expr)))
        .collect();
    let mut columns = keys.clone();
    for selection in &request.aggregates {
        let measure = summed(plan, selection, "aggregates")?;
        columns.push(format!(
            "{} AS {}",
            total(measure),
            generator.quote_identifier(selection.alias())
        ));
    }
    let mut sql = format!(
        "SELECT\n  {}\nFROM (\n{fine_sql}\nUNION ALL\n{coarse_sql}\n) AS cover",
        columns.join(",\n  ")
    );
    if !keys.is_empty() {
        let _ = write!(sql, "\nGROUP BY {}", keys.join(", "));
    }
    let mut having = Vec::with_capacity(request.having.len());
    for condition in &request.having {
        let measure = summed(plan, &condition.aggregate, "having")?;
        let value = generator.emit_value_param(&condition.value, &mut params);
        having.push(format!("{} {} {value}", total(measure), condition.operator.sql_operator()));
    }
    if !having.is_empty() {
        let _ = write!(sql, "\nHAVING {}", having.join(" AND "));
    }
    if !request.order_by.is_empty() {
        let mut order = Vec::with_capacity(request.order_by.len());
        for clause in &request.order_by {
            let key = generator.quote_identifier(&clause.field);
            let key = match clause.collation.as_deref() {
                Some(collation) => crate::backend::order_by::collated(&key, Some(collation))?,
                None => key,
            };
            order.push(format!("{key} {}", clause.direction_sql()));
        }
        let _ = write!(sql, "\nORDER BY {}", order.join(", "));
    }
    if let Some(limit) = request.limit {
        let _ = write!(sql, "\nLIMIT {limit}");
    }
    if let Some(offset) = request.offset {
        let _ = write!(sql, "\nOFFSET {offset}");
    }
    Ok(ParameterizedAggregationSql { sql, params })
}
