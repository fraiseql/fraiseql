//! Nested resource embedding: `?select=` sub-selects, in the engine's terms.
//!
//! Supports `OneToMany` (array), `ManyToOne` (single object) and `OneToOne` (object or
//! null) relationships, and `rel.count` totals. Empty collections return `[]`, not null;
//! absent single objects return `null`.
//!
//! **How an embed is resolved.** In one SQL statement, by the engine:
//! [`Executor::execute_query_composed`](fraiseql_core::runtime::Executor::execute_query_composed)
//! joins each embedded level into its parent's statement as a correlated `LATERAL`
//! subquery with its own ordering and page, and gates every level as the read of its own
//! target it replaces. This module only translates the parsed `?select=` into that
//! request ([`EmbedSelection`], [`CountSelection`]); it reads nothing itself.
//!
//! It used to read everything itself: one sub-read per parent row per level, with the
//! join key read off each already-projected parent row. The reads multiplied with the
//! page size at each level, which is what `[rest] max_embedded_reads` existed to bound,
//! and the join key had to be projected for the server and stripped again (#1230). Both
//! are gone with the fan-out: the correlation is a column reference in SQL, and the
//! statement is bounded by the request's cost and bytes ceilings like any other read.

use std::collections::HashMap;

use fraiseql_core::runtime::{CountSelection, EmbedSelection};

use super::params::{EmbeddedSpec, SelectEntry};

#[cfg(test)]
mod tests;

/// A sub-select's entries, separated by kind.
///
/// The whole of `?select=posts(title,author(name),comments.count)` after the
/// parser: flat fields, nested embeds, nested counts.
///
/// **Why this type exists rather than three `filter_map`s.** A sub-select used
/// to be read by a single `filter_map` matching `SelectEntry::Field` with
/// `_ => None` for the rest, so nested embeds and nested counts were parsed,
/// depth-validated and then silently discarded — the response simply lacked the
/// key, under a 200, and a client could not tell "nothing related" from "the
/// server dropped my selection". #864 fixed the embed half and left the count
/// half in the wildcard, with a comment that named both. #1267 is that second
/// half, found three releases later.
///
/// [`Self::split`] matches **exhaustively**: there is no `_` arm, so a fourth
/// `SelectEntry` variant cannot be added without this function failing to
/// compile. That is the point — the two defects above were both a wildcard
/// quietly absorbing a case nobody had handled.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct SubSelect {
    /// Flat field names, in selection order.
    pub fields: Vec<String>,
    /// Nested embedded resources, in selection order.
    pub embeds: Vec<EmbeddedSpec>,
    /// Nested count-only relationships, in selection order.
    pub counts: Vec<String>,
}

impl SubSelect {
    /// Separate `entries` by kind, preserving selection order within each.
    pub(super) fn split(entries: &[SelectEntry]) -> Self {
        let mut out = Self::default();
        for entry in entries {
            match entry {
                SelectEntry::Field(name) => out.fields.push(name.clone()),
                SelectEntry::Embedded(spec) => out.embeds.push(spec.clone()),
                SelectEntry::Count(name) => out.counts.push(name.clone()),
            }
        }
        out
    }
}

/// The key a `rel.count` total is written under.
#[must_use]
pub fn count_output_key(relationship: &str) -> String {
    format!("{relationship}_count")
}

/// The embeds and counts a request selected, as the engine takes them.
///
/// `filters` is the request's `?rel.field=value` map. It is read for the **top level
/// only**, by relationship name, and by an embed and a count of the same relationship
/// alike — so `?select=posts(id),posts.count&posts.status=published` lists and counts
/// the same rows (#1285). The syntax is flat, one segment deep, so no filter can name a
/// nested selection, and the nested levels are given none rather than the parent's.
///
/// `page` is the page each parent row gets of related rows: the deployment's
/// `max_page_size`, as it always was for an embed.
#[must_use]
#[allow(clippy::implicit_hasher)] // Reason: the one caller holds a std `HashMap`
pub fn selections(
    embeddings: &[EmbeddedSpec],
    counts: &[String],
    filters: &HashMap<String, serde_json::Value>,
    page: Option<u32>,
) -> (Vec<EmbedSelection>, Vec<CountSelection>) {
    let no_filters = HashMap::new();
    let embeds = embeddings
        .iter()
        .map(|spec| {
            let SubSelect {
                fields,
                embeds: nested,
                counts: nested_counts,
            } = SubSelect::split(&spec.fields);
            let (embeds, counts) = selections(&nested, &nested_counts, &no_filters, page);
            EmbedSelection {
                relationship: spec.relationship.clone(),
                output_key: spec.rename.clone().unwrap_or_else(|| spec.relationship.clone()),
                fields,
                filter: filters.get(&spec.relationship).cloned(),
                limit: page,
                embeds,
                counts,
            }
        })
        .collect();
    let counts = counts
        .iter()
        .map(|relationship| CountSelection {
            relationship: relationship.clone(),
            output_key:   count_output_key(relationship),
            filter:       filters.get(relationship).cloned(),
        })
        .collect();
    (embeds, counts)
}
