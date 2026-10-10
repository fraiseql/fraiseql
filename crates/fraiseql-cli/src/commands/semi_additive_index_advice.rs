//! Index advice for a measure carried forward per entity (#1459).
//!
//! An aggregate over a `semi_additive` measure reduced by `last` or `first` reads one row
//! per (bucket, entity) cell: the entity's last row before the bucket's end, found by an
//! index seek on `(entity…, over)`. Without that index each cell scans the entity's rows,
//! and the request's cost grows with the table rather than with its cells. The measures
//! reduced within the bucket (`avg`, `min`, `max`, `delta`) read the range once, grouped,
//! and need no seek.
//!
//! Pure, like [`super::pagination_index_advice`]: the catalog reads live in `doctor`.

use fraiseql_core::compiler::{
    fact_table::{Additivity, FactTableMetadata},
    time_reduction::Reduction,
};
use fraiseql_db::postgres::IndexInfo;

use super::pagination_index_advice::keys_match;

/// A seek key no index on the fact table leads with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeekAdvice {
    /// The measures carried forward over this key.
    pub measures: Vec<String>,
    /// The key, entity columns first, then the time column.
    pub key:      Vec<String>,
    /// The `CREATE INDEX` statement that serves it.
    pub ddl:      String,
}

/// Whether an `additivity` is reduced by an index seek per cell: the planner's carried-forward
/// reductions.
fn seeks(additivity: &Additivity) -> bool {
    Reduction::of(additivity).is_some_and(Reduction::carries_forward)
}

/// The seek keys of `fact`'s carried-forward measures that no index on `relation` leads
/// with, each with its DDL.
#[must_use]
pub fn advise(fact: &FactTableMetadata, relation: &str, indexes: &[IndexInfo]) -> Vec<SeekAdvice> {
    let mut advice: Vec<SeekAdvice> = Vec::new();
    for measure in &fact.measures {
        let (true, Additivity::SemiAdditive { over, entity, .. }) =
            (seeks(&measure.additivity), &measure.additivity)
        else {
            continue;
        };
        let key: Vec<String> = entity.iter().chain(std::iter::once(over)).cloned().collect();
        let served = indexes.iter().any(|ix| {
            ix.keys.len() >= key.len() && key.iter().zip(&ix.keys).all(|(k, i)| keys_match(k, i))
        });
        if served {
            continue;
        }
        if let Some(found) = advice.iter_mut().find(|a| a.key == key) {
            found.measures.push(measure.name.clone());
        } else {
            advice.push(SeekAdvice {
                measures: vec![measure.name.clone()],
                ddl: format!("CREATE INDEX ON {relation} ({});", key.join(", ")),
                key,
            });
        }
    }
    advice
}

/// Whether `fact` declares a measure an index seek reduces, so it has seek keys to check.
#[must_use]
pub fn has_carried_forward_measure(fact: &FactTableMetadata) -> bool {
    fact.measures.iter().any(|m| seeks(&m.additivity))
}
