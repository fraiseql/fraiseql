//! pg_tviews' change feed, as the result cache reads it (#1589).
//!
//! With `pg_tviews.publish_changes = 'rows'`, every committed transaction that changed TVIEW
//! rows says which on the PostgreSQL channel [`CHANNEL`], whoever ran the write: FraiseQL, a
//! script, another application. This module reads one payload of version 2 into a
//! [`TviewsMessage`].
//!
//! ```json
//! {"v":2,"u":{"Comment":["c1"]},"d":{"Tag":["t1"]},"c":{"Comment.fk_post":["p1"]},"t":["Audit"]}
//! ```
//!
//! `u` and `d` are the rows inserted or updated and deleted, by GraphQL type and public id;
//! `c` the collections whose members changed, by `<child type>.<reference column>` and parent
//! id; `t` the types whose rows or collections are not listed. Anything this reader cannot
//! read evicts more, never less: an id it cannot read evicts its type, a message of another
//! version or of another shape evicts everything.

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;
use serde_json::Value;

/// The `NOTIFY` channel pg_tviews publishes on.
pub const CHANNEL: &str = "tviews_changes";

/// The payload version this reader knows.
const VERSION: u64 = 2;

/// A collection of child rows: those of `child_type` whose `column` holds a parent's key.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Collection {
    /// The children's GraphQL type (`Comment`).
    pub child_type: String,
    /// The children's reference column (`fk_post`).
    pub column:     String,
}

impl Collection {
    /// The collection of `child_type` rows by `column`.
    #[must_use]
    pub fn new(child_type: impl Into<String>, column: impl Into<String>) -> Self {
        Self {
            child_type: child_type.into(),
            column:     column.into(),
        }
    }
}

/// What one message says changed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TviewsChange {
    /// Rows inserted, updated or deleted: public ids by type.
    pub rows:        BTreeMap<String, BTreeSet<String>>,
    /// Collections whose members changed: parent ids by collection.
    pub collections: BTreeMap<Collection, BTreeSet<String>>,
    /// Types whose rows and collections are not listed: all of them changed.
    pub types:       BTreeSet<String>,
}

/// One payload received on [`CHANNEL`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TviewsMessage {
    /// A message this reader understands.
    Change(TviewsChange),
    /// Anything else: every entry may be stale.
    Everything,
}

/// A message on the wire. Every key but `v` may be absent.
#[derive(Deserialize)]
struct Wire {
    v: Option<u64>,
    #[serde(default)]
    u: BTreeMap<String, Vec<Value>>,
    #[serde(default)]
    d: BTreeMap<String, Vec<Value>>,
    #[serde(default)]
    c: BTreeMap<String, Vec<Value>>,
    #[serde(default)]
    t: Vec<String>,
}

/// Read a payload received on [`CHANNEL`].
#[must_use]
pub fn decode(payload: &str) -> TviewsMessage {
    let Ok(wire) = serde_json::from_str::<Wire>(payload) else {
        return TviewsMessage::Everything;
    };
    if wire.v != Some(VERSION) {
        return TviewsMessage::Everything;
    }
    let mut change = TviewsChange {
        types: wire.t.into_iter().collect(),
        ..TviewsChange::default()
    };
    for (type_name, ids) in wire.u.into_iter().chain(wire.d) {
        match ids_of(&ids) {
            Some(ids) => change.rows.entry(type_name).or_default().extend(ids),
            None => {
                change.types.insert(type_name);
            },
        }
    }
    for (key, parents) in wire.c {
        let Some(collection) = collection_of(&key) else {
            return TviewsMessage::Everything;
        };
        match ids_of(&parents) {
            Some(ids) => change.collections.entry(collection).or_default().extend(ids),
            None => {
                change.types.insert(collection.child_type);
            },
        }
    }
    TviewsMessage::Change(change)
}

/// The ids `values` hold, each as text, or `None` when one is neither text nor a number.
fn ids_of(values: &[Value]) -> Option<Vec<String>> {
    values
        .iter()
        .map(|value| match value {
            Value::String(id) => Some(id.clone()),
            Value::Number(id) => Some(id.to_string()),
            _ => None,
        })
        .collect()
}

/// `Comment.fk_post` as its child type and column: the type is a GraphQL name, which holds
/// no dot, so the first dot splits them.
fn collection_of(key: &str) -> Option<Collection> {
    let (child_type, column) = key.split_once('.')?;
    (!child_type.is_empty() && !column.is_empty()).then(|| Collection::new(child_type, column))
}

#[cfg(test)]
#[path = "tviews_feed_tests.rs"]
mod tviews_feed_tests;
