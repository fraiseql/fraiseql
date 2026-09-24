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

use std::collections::{BTreeMap, HashMap};

use fraiseql_core::{
    runtime::{CountSelection, EmbedSelection},
    schema::RestConfig,
};

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

/// The dotted path of the level `relationship` under the level at `prefix` — the key a
/// `?orders.items.limit=` parameter is stored under.
#[must_use]
pub fn level_path(prefix: &str, relationship: &str) -> String {
    if prefix.is_empty() {
        relationship.to_string()
    } else {
        format!("{prefix}.{relationship}")
    }
}

/// Every level path a `?select=` embeds, rows only — a `rel.count` has no page.
#[must_use]
pub fn embedded_level_paths(embeddings: &[EmbeddedSpec]) -> Vec<String> {
    fn walk(prefix: &str, embeddings: &[EmbeddedSpec], out: &mut Vec<String>) {
        for spec in embeddings {
            let path = level_path(prefix, &spec.relationship);
            walk(&path, &SubSelect::split(&spec.fields).embeds, out);
            out.push(path);
        }
    }
    let mut out = Vec::new();
    walk("", embeddings, &mut out);
    out
}

/// The page an embedded level gets when the request names none: `default_embed_page_size`,
/// applied at most at `max_page_size`.
///
/// Capped rather than refused because nobody asked for it: a deployment that lowers the
/// ceiling below the default has not asked every unpaged embed to fail, and a client's own
/// `?rel.limit=` above the ceiling is refused where it is parsed.
#[must_use]
pub fn default_embed_page(config: &RestConfig) -> u32 {
    u32::try_from(config.default_embed_page_size.min(config.max_page_size)).unwrap_or(u32::MAX)
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
/// `pages` is the page each level asked for, keyed by its dotted relationship path
/// (`?orders.limit=`, `?orders.items.limit=`); a level it does not name gets
/// `default_page`, the deployment's `default_embed_page_size`. Unlike a filter, a page
/// reaches nested levels, because its path names them — and the parent's `?limit=` is
/// never one: it pages the parent.
#[must_use]
#[allow(clippy::implicit_hasher)] // Reason: the one caller holds a std `HashMap`
pub fn selections(
    embeddings: &[EmbeddedSpec],
    counts: &[String],
    filters: &HashMap<String, serde_json::Value>,
    pages: &BTreeMap<String, u32>,
    default_page: u32,
) -> (Vec<EmbedSelection>, Vec<CountSelection>) {
    selections_at("", embeddings, counts, filters, pages, default_page)
}

/// [`selections`] for the levels under `prefix`, the dotted path of their parent level.
fn selections_at(
    prefix: &str,
    embeddings: &[EmbeddedSpec],
    counts: &[String],
    filters: &HashMap<String, serde_json::Value>,
    pages: &BTreeMap<String, u32>,
    default_page: u32,
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
            let path = level_path(prefix, &spec.relationship);
            let (embeds, counts) =
                selections_at(&path, &nested, &nested_counts, &no_filters, pages, default_page);
            EmbedSelection {
                relationship: spec.relationship.clone(),
                output_key: spec.rename.clone().unwrap_or_else(|| spec.relationship.clone()),
                fields,
                filter: filters.get(&spec.relationship).cloned(),
                limit: Some(pages.get(&path).copied().unwrap_or(default_page)),
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
