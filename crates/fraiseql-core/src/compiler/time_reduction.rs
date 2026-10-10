//! How an aggregate over a measure that is not additive over time is answered (#1459).
//!
//! A balance or a stock level is **semi-additive**: summed across accounts it means
//! something, summed across days it does not. Such a measure is first reduced per entity and
//! per bucket of the request's time grouping, then the requested function runs across
//! entities:
//!
//! | Declared | Per entity, per bucket | Across entities |
//! |---|---|---|
//! | `semi_additive(using = last)` | the last value known by the bucket's end, carried forward | requested function |
//! | `semi_additive(using = first)` | the first value in the bucket, else the value carried in | requested function |
//! | `semi_additive(using = avg \| min \| max)` | over the bucket's own rows | requested function |
//! | `delta` | last − first of the bucket's own rows | requested function |
//! | `non_additive` | refused | refused |
//!
//! A request the reduction cannot answer is refused, never approximated.

use serde::{Deserialize, Serialize};

use crate::{
    backend::where_clause::{WhereClause, WhereOperator},
    compiler::{
        aggregate_types::{AggregateFunction, TemporalBucket},
        aggregation::{AggregateSelection, AggregationRequest, GroupBySelection},
        fact_table::{Additivity, FactTableMetadata, SemiAdditiveReduction, SqlType},
        window_functions::{WindowFunctionSpec, WindowRequest},
    },
    error::{FraiseQLError, Result},
};

/// The per-entity, per-bucket reduction an aggregate plan runs before the requested
/// function (#1459).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeReduction {
    /// The time column the measures are reduced over.
    pub over:         String,
    /// Whether `over` is a `date` (`true`) or a timestamp column.
    pub over_date:    bool,
    /// The columns that identify what a value belongs to.
    pub entity:       Vec<String>,
    /// The grain of the request's time grouping, which is the bucket reduced over.
    pub bucket:       TemporalBucket,
    /// The output alias of that time grouping.
    pub bucket_alias: String,
    /// How each entity's rows in a bucket become one value.
    pub reduction:    Reduction,
    /// The measures reduced, by name, in the order their reduced values are selected.
    pub measures:     Vec<String>,
}

/// How an entity's rows in a bucket become one value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Reduction {
    /// The last value known by the bucket's end, carried forward from an earlier bucket.
    Last,
    /// The first value in the bucket, else the value carried in from an earlier bucket.
    First,
    /// The average of the bucket's own rows.
    Avg,
    /// The least of the bucket's own rows.
    Min,
    /// The greatest of the bucket's own rows.
    Max,
    /// The last of the bucket's own rows minus the first.
    Delta,
}

impl Reduction {
    /// Whether a bucket with no row of an entity takes the value carried from before it.
    #[must_use]
    pub const fn carries_forward(self) -> bool {
        match self {
            Self::Last | Self::First => true,
            Self::Avg | Self::Min | Self::Max | Self::Delta => false,
        }
    }

    /// The reduction a declared `additivity` runs per entity and bucket; `None` for one with
    /// no reduction (`additive`, or `non_additive`, which is refused).
    #[must_use]
    pub const fn of(additivity: &Additivity) -> Option<Self> {
        match additivity {
            Additivity::SemiAdditive { using, .. } => Some(match using {
                SemiAdditiveReduction::Last => Self::Last,
                SemiAdditiveReduction::First => Self::First,
                SemiAdditiveReduction::Avg => Self::Avg,
                SemiAdditiveReduction::Min => Self::Min,
                SemiAdditiveReduction::Max => Self::Max,
            }),
            Additivity::Delta { .. } => Some(Self::Delta),
            Additivity::Additive | Additivity::NonAdditive => None,
        }
    }
}

/// The declared additivity of the measure a request names, `Additive` for a name that is
/// not a declared measure (a dimension, a filter column or a native measure mapping).
fn additivity_of<'a>(metadata: &'a FactTableMetadata, name: &str) -> &'a Additivity {
    const ADDITIVE: &Additivity = &Additivity::Additive;
    metadata
        .measures
        .iter()
        .find(|m| m.name == name)
        .map_or(ADDITIVE, |m| &m.additivity)
}

const fn refusal(message: String) -> FraiseQLError {
    FraiseQLError::Validation {
        message,
        path: None,
    }
}

/// The measure an aggregate selection reads, if any.
fn measure_read(selection: &AggregateSelection) -> Option<&str> {
    match selection {
        AggregateSelection::MeasureAggregate { measure, .. } => Some(measure),
        AggregateSelection::CountDistinct { field, .. } => Some(field),
        AggregateSelection::Count { .. } | AggregateSelection::BoolAggregate { .. } => None,
    }
}

/// Plan the reduction a request over measures that are not additive needs, or refuse the
/// request when the reduction cannot answer it.
///
/// # Errors
///
/// `FraiseQLError::Validation`, naming the measure, when the request aggregates a
/// `non_additive` measure; combines a reduced measure with any other aggregate, or with a
/// measure reduced differently; reads a reduced measure through a function other than a
/// plain aggregate; does not group by exactly one time bucket of the measure's `over`; or,
/// for a carried-forward measure, filters `over` other than by a conjunction of comparisons.
pub fn plan_time_reduction(
    request: &AggregationRequest,
    metadata: &FactTableMetadata,
) -> Result<Option<TimeReduction>> {
    let selections: Vec<&AggregateSelection> = request
        .aggregates
        .iter()
        .chain(request.having.iter().map(|h| &h.aggregate))
        .collect();

    let mut declared: Option<(&str, &Additivity)> = None;
    for selection in &selections {
        let Some(measure) = measure_read(selection) else {
            continue;
        };
        let additivity = additivity_of(metadata, measure);
        match additivity {
            Additivity::Additive => {},
            Additivity::NonAdditive => {
                return Err(refusal(format!(
                    "measure `{measure}` of fact table `{}` is declared non-additive: no \
                     aggregate over it is meaningful",
                    metadata.table_name
                )));
            },
            // The first one read sets the reduction; the loop below refuses any other.
            Additivity::SemiAdditive { .. } | Additivity::Delta { .. } => {
                declared.get_or_insert((measure, additivity));
            },
        }
    }
    let Some((measure, additivity)) = declared else {
        return Ok(None);
    };
    let (
        Some(reduction),
        Additivity::SemiAdditive { over, entity, .. } | Additivity::Delta { over, entity },
    ) = (Reduction::of(additivity), additivity)
    else {
        return Ok(None);
    };

    // Every aggregate reads a measure reduced the same way, through a plain aggregate.
    let mut measures: Vec<String> = Vec::new();
    for selection in &selections {
        let reduced = match selection {
            AggregateSelection::MeasureAggregate {
                measure: name,
                function,
                ..
            } => {
                if additivity_of(metadata, name) != additivity {
                    return Err(refusal(format!(
                        "`{name}` cannot be aggregated with `{measure}`, which is reduced per \
                         `{}` and per bucket of `{over}` first; request it in a separate \
                         aggregate",
                        entity.join("`, `")
                    )));
                }
                if matches!(
                    function,
                    AggregateFunction::ArrayAgg
                        | AggregateFunction::JsonAgg
                        | AggregateFunction::JsonbAgg
                        | AggregateFunction::StringAgg
                ) {
                    return Err(refusal(format!(
                        "`{measure}` is reduced per bucket of `{over}`; {function:?} over it is \
                         not supported"
                    )));
                }
                name
            },
            other => {
                return Err(refusal(format!(
                    "`{}` cannot be combined with `{measure}`, which is reduced per `{}` and \
                     per bucket of `{over}` first; request it in a separate aggregate",
                    other.alias(),
                    entity.join("`, `")
                )));
            },
        };
        if !measures.iter().any(|m| m == reduced) {
            measures.push(reduced.clone());
        }
    }

    // Grouped by exactly one time bucket, of `over`.
    let mut temporal = request.group_by.iter().filter(|g| {
        matches!(
            g,
            GroupBySelection::TemporalBucket { .. } | GroupBySelection::CalendarDimension { .. }
        )
    });
    let (bucket, bucket_alias) = match (temporal.next(), temporal.next()) {
        (
            Some(GroupBySelection::TemporalBucket {
                column,
                bucket,
                alias,
            }),
            None,
        ) if column == over => (*bucket, alias.clone()),
        _ => {
            return Err(refusal(format!(
                "`{measure}` is reduced per bucket of `{over}`: group by exactly one time \
                 bucket of `{over}` (a day, week, month, quarter or year of it)"
            )));
        },
    };

    let over_date = metadata
        .denormalized_filters
        .iter()
        .find(|f| f.name == *over)
        .is_some_and(|f| f.sql_type == SqlType::Date);

    if reduction.carries_forward() {
        if let Some(clause) = &request.where_clause {
            time_bounds(clause, over).ok_or_else(|| {
                refusal(format!(
                    "`{measure}` carries each `{}`'s last value forward over `{over}`: filter \
                     `{over}` only by comparisons (eq, gt, gte, lt, lte) joined by and, never \
                     under or / not",
                    entity.join("`, `")
                ))
            })?;
        }
    }

    Ok(Some(TimeReduction {
        over: over.clone(),
        over_date,
        entity: entity.clone(),
        bucket,
        bucket_alias,
        reduction,
        measures,
    }))
}

/// A comparison bounding the time column, from the request's top-level conjunction.
#[derive(Debug, Clone, PartialEq)]
pub struct TimeBound<'a> {
    /// The comparison: `Eq`, `Gt`, `Gte`, `Lt` or `Lte`.
    pub operator: &'a WhereOperator,
    /// The value compared against.
    pub value:    &'a serde_json::Value,
}

/// A request filter split for a carried-forward reduction: the comparisons on the time
/// column, and every other conjunct.
#[derive(Debug, Default)]
pub struct SplitFilter<'a> {
    /// The comparisons on the time column.
    pub bounds: Vec<TimeBound<'a>>,
    /// Every other conjunct, which never names the time column.
    pub rest:   Vec<&'a WhereClause>,
}

/// Split `clause` into the comparisons on `over` and the rest, or `None` when `over` appears
/// anywhere but in a top-level comparison (under `or` or `not`, or compared otherwise).
#[must_use]
pub fn time_bounds<'a>(clause: &'a WhereClause, over: &str) -> Option<SplitFilter<'a>> {
    let mut split = SplitFilter::default();
    let mut conjuncts = vec![clause];
    while let Some(conjunct) = conjuncts.pop() {
        match conjunct {
            WhereClause::And(clauses) => conjuncts.extend(clauses.iter()),
            WhereClause::Typed { inner, .. } => conjuncts.push(inner),
            WhereClause::NativeField {
                column,
                operator,
                value,
                ..
            } if column == over => split.bounds.push(bound(operator, value)?),
            WhereClause::Field {
                path,
                operator,
                value,
            } if matches!(path.as_slice(), [only] if only == over) => {
                split.bounds.push(bound(operator, value)?);
            },
            other if mentions(other, over) => return None,
            other => split.rest.push(other),
        }
    }
    Some(split)
}

fn bound<'a>(operator: &'a WhereOperator, value: &'a serde_json::Value) -> Option<TimeBound<'a>> {
    match operator {
        WhereOperator::Eq
        | WhereOperator::Gt
        | WhereOperator::Gte
        | WhereOperator::Lt
        | WhereOperator::Lte
            if !value.is_null() =>
        {
            Some(TimeBound { operator, value })
        },
        _ => None,
    }
}

/// Whether `clause` reads the column `over` anywhere. A clause kind this does not know is
/// taken to read it, so a carried-forward reduction refuses it rather than mis-bound it.
fn mentions(clause: &WhereClause, over: &str) -> bool {
    match clause {
        WhereClause::Field { path, .. } => path.first().is_some_and(|p| p == over),
        WhereClause::NativeField { column, .. } => column == over,
        WhereClause::And(clauses) | WhereClause::Or(clauses) => {
            clauses.iter().any(|c| mentions(c, over))
        },
        WhereClause::Not(inner)
        | WhereClause::Typed { inner, .. }
        | WhereClause::InHierarchy { inner, .. }
        | WhereClause::Localized { inner, .. } => mentions(inner, over),
        WhereClause::Guarded { guard, inner, .. } => mentions(guard, over) || mentions(inner, over),
        // Reason: `WhereClause` is non_exhaustive across crates; an unknown kind is refused.
        _ => true,
    }
}

/// Refuse a window aggregate over a measure not declared additive (#1459).
///
/// A window runs over fact rows, so a running sum of a balance adds the same balance once
/// per day. A
/// value function (`lag`, `lead`, `first`/`last`/`nth` value) or a plain selection reads one
/// row's value and combines nothing, and a running count counts rows.
///
/// # Errors
///
/// `FraiseQLError::Validation` naming the measure.
pub fn refuse_window_over_non_additive(
    request: &WindowRequest,
    metadata: &FactTableMetadata,
) -> Result<()> {
    for window in &request.windows {
        let measure = match &window.function {
            WindowFunctionSpec::RunningSum { measure }
            | WindowFunctionSpec::RunningAvg { measure }
            | WindowFunctionSpec::RunningMin { measure }
            | WindowFunctionSpec::RunningMax { measure }
            | WindowFunctionSpec::RunningStddev { measure }
            | WindowFunctionSpec::RunningVariance { measure } => measure,
            WindowFunctionSpec::RowNumber
            | WindowFunctionSpec::Rank
            | WindowFunctionSpec::DenseRank
            | WindowFunctionSpec::Ntile { .. }
            | WindowFunctionSpec::PercentRank
            | WindowFunctionSpec::CumeDist
            | WindowFunctionSpec::Lag { .. }
            | WindowFunctionSpec::Lead { .. }
            | WindowFunctionSpec::FirstValue { .. }
            | WindowFunctionSpec::LastValue { .. }
            | WindowFunctionSpec::NthValue { .. }
            | WindowFunctionSpec::RunningCount
            | WindowFunctionSpec::RunningCountField { .. } => continue,
        };
        if *additivity_of(metadata, measure) != Additivity::Additive {
            return Err(refusal(format!(
                "measure `{measure}` of fact table `{}` is not additive over time: a window \
                 aggregate runs over fact rows and would combine it across them; use the \
                 aggregate query, which reduces it per entity and per bucket first",
                metadata.table_name
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
