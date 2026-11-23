//! A rolled-up fact table linked to a finer one over the same rows (`grain.edges_from`).
//!
//! A range that would split one of the coarse table's rows is answered from both: whole
//! periods from the coarse table, the days around them from the finer one. That is only
//! the coarse table's answer when the two tables are one table at two grains: the same
//! measures, dimensions and filters, read under the same gates, the finer grain's rows
//! nesting in the coarser's. Each is checked here, when the schema is compiled or loaded.

use std::{collections::HashMap, hash::BuildHasher};

use super::{FactGrain, FactTableMetadata};

/// One message per fault in a fact table's `grain.edges_from`, tables in name order.
#[must_use]
pub fn linked_grain_violations<S: BuildHasher>(
    fact_tables: &HashMap<String, FactTableMetadata, S>,
) -> Vec<String> {
    let mut linked: Vec<(&FactTableMetadata, &FactGrain, &str)> = fact_tables
        .values()
        .filter_map(|table| {
            let grain = table.grain.as_ref()?;
            Some((table, grain, grain.edges_from.as_deref()?))
        })
        .collect();
    linked.sort_by(|a, b| a.0.table_name.cmp(&b.0.table_name));
    let mut violations = Vec::new();
    for (coarse, grain, name) in linked {
        let prefix = format!("fact table `{}`: `grain.edges_from` `{name}`", coarse.table_name);
        let mut fault = |why: String| violations.push(format!("{prefix} {why}"));
        if name == coarse.table_name {
            fault("names the table itself".to_string());
            continue;
        }
        let Some(fine) = fact_tables.get(name) else {
            fault("is not a fact table of the schema".to_string());
            continue;
        };
        for why in pair_faults(coarse, grain, fine) {
            fault(why);
        }
    }
    violations
}

/// Whether `fine` answers the edges of `coarse`'s ranges.
///
/// `coarse` declares a grain, and the pair has none of the faults a loaded schema is
/// refused for. A hand-built schema is
/// held to the same rule at request time, so a pair that was never checked is never read.
#[must_use]
pub fn edges_answerable(coarse: &FactTableMetadata, fine: &FactTableMetadata) -> bool {
    coarse.grain.as_ref().is_some_and(|grain| {
        grain.edges_from.as_deref() == Some(fine.table_name.as_str())
            && fine.table_name != coarse.table_name
            && pair_faults(coarse, grain, fine).is_empty()
    })
}

/// Why `fine` cannot answer the edges of `coarse`'s ranges, one reason per fault.
fn pair_faults(
    coarse: &FactTableMetadata,
    grain: &FactGrain,
    fine: &FactTableMetadata,
) -> Vec<String> {
    let mut faults = Vec::new();
    match &fine.grain {
        None => faults.push(format!(
            "declares no `grain`; the edges are read from a table that declares one on `{}`",
            grain.column
        )),
        Some(edges) => {
            if edges.column != grain.column {
                faults.push(format!(
                    "declares its grain on `{}`, and this table on `{}`; both must be on one \
                     column",
                    edges.column, grain.column
                ));
            }
            if edges.bucket == grain.bucket || !edges.bucket.serves(grain.bucket.temporal()) {
                faults.push(format!(
                    "holds one row per {}, which is not a finer period nesting in a {}: the \
                     edges of a {} must be whole rows of the finer table",
                    period(edges),
                    period(grain),
                    period(grain)
                ));
            }
            if let Some(next) = &edges.edges_from {
                faults.push(format!(
                    "links edges of its own (`{next}`); the edges are read from one finer \
                     table, not a chain"
                ));
            }
        },
    }
    faults.extend(measure_faults(coarse, fine));
    faults.extend(filter_faults(coarse, fine));
    if coarse.dimensions != fine.dimensions {
        faults.push("declares other `dimensions`".to_string());
    }
    if coarse.calendar_dimensions != fine.calendar_dimensions {
        faults.push("declares other `calendar_dimensions`".to_string());
    }
    if coarse.native_dimension_mapping != fine.native_dimension_mapping {
        faults.push("declares another `native_dimension_mapping`".to_string());
    }
    if coarse.type_name != fine.type_name
        || coarse.row_scope != fine.row_scope
        || coarse.inject_params != fine.inject_params
    {
        faults.push(format!(
            "is read under other gates than `{}` (`type_name`, `row_scope` and \
             `inject_params` must be equal): its rows would answer callers under rules they \
             are not held to",
            coarse.table_name
        ));
    }
    faults
}

/// A grain's period as a word: `half-month`.
fn period(grain: &FactGrain) -> String {
    grain.bucket.name().replace('_', "-")
}

/// The measures the two tables do not declare alike. A measure's stored `column` may
/// differ: each table is read from its own.
fn measure_faults(coarse: &FactTableMetadata, fine: &FactTableMetadata) -> Vec<String> {
    let mut names: Vec<&str> =
        coarse.measures.iter().chain(&fine.measures).map(|m| m.name.as_str()).collect();
    names.sort_unstable();
    names.dedup();
    names
        .into_iter()
        .filter_map(|name| match (coarse.measure(name), fine.measure(name)) {
            (Some(a), Some(b))
                if a.sql_type == b.sql_type
                    && a.nullable == b.nullable
                    && a.additivity == b.additivity =>
            {
                None
            },
            (Some(_), Some(_)) => Some(format!(
                "declares measure `{name}` with another type, nullability or additivity"
            )),
            _ => Some(format!("measure `{name}` is declared on only one of the two tables")),
        })
        .collect()
}

/// The denormalized filters the two tables do not declare alike: the sets must be equal,
/// so the finer table never answers for a name the coarse table's root does not publish.
fn filter_faults(coarse: &FactTableMetadata, fine: &FactTableMetadata) -> Vec<String> {
    let find = |table: &FactTableMetadata, name: &str| {
        table.denormalized_filters.iter().find(|f| f.name == name).cloned()
    };
    let mut names: Vec<&str> = coarse
        .denormalized_filters
        .iter()
        .chain(&fine.denormalized_filters)
        .map(|f| f.name.as_str())
        .collect();
    names.sort_unstable();
    names.dedup();
    names
        .into_iter()
        .filter_map(|name| match (find(coarse, name), find(fine, name)) {
            (Some(a), Some(b)) if a == b => None,
            (Some(_), Some(_)) => {
                Some(format!("declares denormalized filter `{name}` with another definition"))
            },
            _ => Some(format!(
                "denormalized filter `{name}` is declared on only one of the two tables"
            )),
        })
        .collect()
}

#[cfg(test)]
#[path = "linked_tests.rs"]
mod tests;
