//! What a cached composed read is indexed under (#1589).
//!
//! A composed read spans every view its levels name. The plain rule indexes the entry under
//! all of them, so a write to any view in the tree evicts it. A level listing the children
//! of each parent row — the comments of each post — can be indexed more precisely when
//! `pg_tviews`' change feed is heard: by its **collections**, `Comment.fk_post:<post id>` for
//! each post in the result, which the feed names whenever a comment joins, leaves or changes
//! in a post's list, whoever wrote it. A comment written under another post then no longer
//! evicts the entry.
//!
//! Which levels are collections comes from the database, never from a name: a level that
//! joins its parent on the native columns `compile --database` proved (#1587), where the
//! child column is a `tviews.registry.references` entry of the level's TVIEW naming the parent
//! level's TVIEW and its identity ([`CollectionMap`]). Every other level — a to-one embed, a
//! join on document keys, a plain view, a parent row without an `id` — keeps the plain rule.

use std::{
    collections::HashMap,
    sync::{Arc, PoisonError, RwLock},
};

use serde_json::Value;

use super::{result::ComposedTags, tviews_feed::Collection};
use crate::{
    backend::{
        COMPOSED_DOCUMENT_KEY, COMPOSED_EMBEDS_KEY, ComposedEmbed, ComposedLevel, EmbedShape,
        EmbedSource, NativeCorrelation,
    },
    db::types::JsonbValue,
};

/// The collections composed levels are, by `(child view, parent view, child column, parent
/// column)`, the views as FraiseQL names them: one entry per registry reference.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CollectionMap {
    levels: HashMap<(String, String, String, String), Collection>,
}

impl CollectionMap {
    /// A map of the given `((child view, parent view, child column, parent column),
    /// collection)` pairs.
    #[must_use]
    pub fn from_levels<'a>(
        levels: impl IntoIterator<Item = ((&'a str, &'a str, &'a str, &'a str), Collection)>,
    ) -> Self {
        Self {
            levels: levels
                .into_iter()
                .map(|((child, parent, column, key), c)| {
                    ((child.into(), parent.into(), column.into(), key.into()), c)
                })
                .collect(),
        }
    }

    /// The collection rows of `child_view` form under a row of `parent_view` when joined on
    /// `join`, if the registry proves one.
    #[must_use]
    pub fn collection(
        &self,
        child_view: &str,
        parent_view: &str,
        join: &NativeCorrelation,
    ) -> Option<&Collection> {
        self.levels.get(&(
            child_view.to_string(),
            parent_view.to_string(),
            join.target_column.clone(),
            join.parent_column.clone(),
        ))
    }
}

/// The [`CollectionMap`] composed reads are indexed by, while this server hears `pg_tviews`'
/// change feed; nothing otherwise (#1589).
///
/// The feed's listener sets it once it listens (after clearing the cache, so no entry
/// predates what it will hear) and clears it the moment it stops (before clearing the cache,
/// whose fence then refuses every fill that read the map while it was set).
#[derive(Debug, Clone, Default)]
pub struct CollectionTagging(Arc<RwLock<Option<Arc<CollectionMap>>>>);

impl CollectionTagging {
    /// The map, while the feed is heard.
    #[must_use]
    pub fn current(&self) -> Option<Arc<CollectionMap>> {
        self.0.read().unwrap_or_else(PoisonError::into_inner).clone()
    }

    /// The feed is heard: index composed reads by `map`'s collections.
    pub fn hear(&self, map: CollectionMap) {
        *self.0.write().unwrap_or_else(PoisonError::into_inner) = Some(Arc::new(map));
    }

    /// The feed is no longer heard: index composed reads by their views only.
    pub fn deafen(&self) {
        *self.0.write().unwrap_or_else(PoisonError::into_inner) = None;
    }
}

/// The views a composed read's entry is indexed under, and its [`ComposedTags`].
///
/// From the read, the rows it returned, and the collections `map` knows (`None`: the change
/// feed is not heard, every level is indexed by its views). `views_for` names the views an
/// entry reading one view is indexed under (the view and the secondary views queries over it
/// declare).
#[must_use]
pub fn composed_tags(
    read: &ComposedLevel,
    rows: &[JsonbValue],
    map: Option<&CollectionMap>,
    views_for: impl Fn(&str) -> Vec<String>,
) -> (Vec<String>, ComposedTags) {
    let mut walk = Walk {
        map,
        views_for: &views_for,
        views: views_for(&read.view),
        tags: ComposedTags::default(),
    };
    let rows: Vec<&Value> = rows.iter().map(JsonbValue::as_value).collect();
    walk.level(read, &rows);
    let Walk {
        mut views,
        mut tags,
        ..
    } = walk;
    dedup(&mut views);
    dedup(&mut tags.collected_views);
    (views, tags)
}

/// [`composed_tags`]' state, threaded through the levels.
struct Walk<'a, F> {
    map:       Option<&'a CollectionMap>,
    views_for: &'a F,
    views:     Vec<String>,
    tags:      ComposedTags,
}

impl<F: Fn(&str) -> Vec<String>> Walk<'_, F> {
    /// Index the levels embedded in `level`, whose rows in the result are `rows`.
    fn level(&mut self, level: &ComposedLevel, rows: &[&Value]) {
        for embed in &level.embeds {
            if let EmbedSource::Correlated { .. } = embed.source {
                match self.collection(level, embed, rows) {
                    Some(members) => {
                        self.tags.collected_views.extend((self.views_for)(&embed.level.view));
                        self.tags.collections.extend(members);
                    },
                    None => self.views.extend((self.views_for)(&embed.level.view)),
                }
            }
            // A materialised level reads its parent's document, not its view: the parent's
            // index covers it. Its own embeds are indexed like any other.
            if embed.shape != EmbedShape::Count {
                let children: Vec<&Value> = rows
                    .iter()
                    .filter_map(|row| row.get(COMPOSED_EMBEDS_KEY)?.get(&embed.output_key))
                    .flat_map(|value| match value {
                        Value::Array(items) => items.iter().collect(),
                        Value::Object(_) => vec![value],
                        _ => Vec::new(),
                    })
                    .collect();
                self.level(&embed.level, &children);
            }
        }
    }

    /// The collection and parent id of every row of `parent` that `embed` lists the
    /// children of, or `None` when the level cannot be indexed by collection: the change
    /// feed is not heard, the embed is to-one, the map knows no such collection, or a
    /// parent row has no id to name.
    fn collection(
        &self,
        parent: &ComposedLevel,
        embed: &ComposedEmbed,
        rows: &[&Value],
    ) -> Option<Vec<(Collection, String)>> {
        let EmbedSource::Correlated {
            native: Some(join), ..
        } = &embed.source
        else {
            return None;
        };
        if embed.shape == EmbedShape::One {
            return None;
        }
        let collection = self.map?.collection(&embed.level.view, &parent.view, join)?;
        rows.iter().map(|row| Some((collection.clone(), parent_id(row)?))).collect()
    }
}

/// The public id of a composed row's document, as the change feed writes it: a uuid's
/// text, an integer's decimal digits.
fn parent_id(row: &Value) -> Option<String> {
    match row.get(COMPOSED_DOCUMENT_KEY)?.get("id")? {
        Value::String(id) => Some(id.clone()),
        Value::Number(id) => Some(id.to_string()),
        _ => None,
    }
}

/// Drop repeated names, keeping the first of each.
fn dedup(names: &mut Vec<String>) {
    let mut seen = std::collections::HashSet::new();
    names.retain(|name| seen.insert(name.clone()));
}

#[cfg(test)]
#[path = "collections_tests.rs"]
mod collections_tests;
