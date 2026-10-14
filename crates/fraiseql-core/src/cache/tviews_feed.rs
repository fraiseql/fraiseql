//! `pg_tviews`' change feed, as the result cache reads it (#1589).
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

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::Arc,
};

use fraiseql_db::ViewName;
use serde::Deserialize;
use serde_json::Value;

use super::{
    QueryResultCache,
    collections::{CollectionMap, CollectionTagging},
};
use crate::{db::DatabaseAdapter, error::Result, schema::CompiledSchema};

/// The `NOTIFY` channel `pg_tviews` publishes on.
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

/// What the change feed is read against, from the database: the views reading each type,
/// and the collections composed levels are.
#[derive(Debug, Clone, Default)]
pub struct FeedCatalog {
    /// For each GraphQL type, the views (as FraiseQL names them) that read its TVIEW, on
    /// their own or through views.
    pub type_views:  HashMap<String, Vec<ViewName>>,
    /// The collections composed levels are.
    pub collections: CollectionMap,
}

impl FeedCatalog {
    /// A catalog from `(type, view)` rows and `(child view, parent view, child type,
    /// reference column, parent identity column)` rows: one collection per reference, which
    /// a level joining on exactly those columns forms.
    #[must_use]
    pub fn from_rows<'a>(
        type_rows: impl IntoIterator<Item = (&'a str, &'a str)>,
        collection_rows: impl IntoIterator<Item = (&'a str, &'a str, &'a str, &'a str, &'a str)>,
    ) -> Self {
        let mut type_views: HashMap<String, Vec<ViewName>> = HashMap::new();
        for (type_name, view) in type_rows {
            let views = type_views.entry(type_name.to_string()).or_default();
            let view = ViewName::from(view);
            if !views.contains(&view) {
                views.push(view);
            }
        }
        let collections = CollectionMap::from_levels(collection_rows.into_iter().map(
            |(child, parent, child_type, column, key)| {
                ((child, parent, column, key), Collection::new(child_type, column))
            },
        ));
        Self {
            type_views,
            collections,
        }
    }
}

/// Whether the database publishes rows on [`CHANNEL`], and has a registry that names
/// types and references (`typename`, which names collections).
const PUBLISHING_PROBE: &str = "SELECT current_setting('pg_tviews.publish_changes', true) \
     = 'rows' AND EXISTS (SELECT 1 FROM pg_attribute \
     WHERE attrelid = to_regclass('tviews.registry') AND attname = 'typename' \
     AND NOT attisdropped) AS publishing";

/// The registry's TVIEWs, as a CTE following [`reads_cte`](super::embedding_graph).
const REGISTRY: &str = "tv AS (SELECT to_regclass(quote_ident(g.schema) || '.' || \
     quote_ident(g.name))::oid AS rel, g.entity, g.typename, g.\"references\" AS refs \
     FROM tviews.registry g)";

/// Read the [`FeedCatalog`] of `schema`'s views from the database `adapter` serves, or
/// `None` when it does not publish rows on [`CHANNEL`].
///
/// # Errors
///
/// `FraiseQLError::Database` when a catalog query fails.
pub async fn read_feed_catalog<A: DatabaseAdapter + ?Sized>(
    adapter: &A,
    schema: &CompiledSchema,
) -> Result<Option<FeedCatalog>> {
    let publishing = adapter
        .execute_raw_query(PUBLISHING_PROBE)
        .await?
        .first()
        .and_then(|row| row.get("publishing"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !publishing {
        return Ok(None);
    }
    let names = super::swept_view_names(schema);
    if names.is_empty() {
        return Ok(Some(FeedCatalog::default()));
    }
    let reads = super::embedding_graph::reads_cte(&names);
    let types = adapter
        .execute_raw_query(&format!(
            "{reads}, {REGISTRY} \
             SELECT DISTINCT tv.typename AS type_name, s.name AS view \
             FROM reads x JOIN roots s ON s.rel = x.root JOIN tv ON tv.rel = x.rel \
             ORDER BY 1, 2"
        ))
        .await?;
    let collections = adapter
        .execute_raw_query(&format!(
            "{reads}, {REGISTRY} \
             SELECT c.name AS child, p.name AS parent, ct.typename AS child_type, \
                    ref.key AS column_name, ref.value ->> 'column' AS key_name \
             FROM roots c JOIN tv ct ON ct.rel = c.rel \
             CROSS JOIN LATERAL jsonb_each(ct.refs) ref \
             JOIN tv pt ON pt.entity = ref.value ->> 'entity' \
             JOIN roots p ON p.rel = pt.rel"
        ))
        .await?;
    let text = |row: &HashMap<String, Value>, key: &str| {
        row.get(key).and_then(Value::as_str).map(str::to_owned)
    };
    let types: Vec<(String, String)> = types
        .iter()
        .filter_map(|row| Some((text(row, "type_name")?, text(row, "view")?)))
        .collect();
    let collections: Vec<[String; 5]> = collections
        .iter()
        .filter_map(|row| {
            Some([
                text(row, "child")?,
                text(row, "parent")?,
                text(row, "child_type")?,
                text(row, "column_name")?,
                text(row, "key_name")?,
            ])
        })
        .collect();
    Ok(Some(FeedCatalog::from_rows(
        types.iter().map(|(t, v)| (t.as_str(), v.as_str())),
        collections
            .iter()
            .map(|[c, p, t, k, r]| (c.as_str(), p.as_str(), t.as_str(), k.as_str(), r.as_str())),
    )))
}

/// Applies `pg_tviews`' change feed to this server's cache.
pub struct TviewsEvictor {
    cache:      Arc<QueryResultCache>,
    type_views: HashMap<String, Vec<ViewName>>,
}

impl TviewsEvictor {
    /// An evictor for `cache`, sweeping for each type the views `type_views` say read it.
    #[must_use]
    pub const fn new(
        cache: Arc<QueryResultCache>,
        type_views: HashMap<String, Vec<ViewName>>,
    ) -> Self {
        Self { cache, type_views }
    }

    /// Evict what `message` says changed.
    ///
    /// # Errors
    ///
    /// Infallible today; the cache's eviction methods keep a `Result` for API compatibility.
    pub fn apply(&self, message: &TviewsMessage) -> Result<()> {
        let TviewsMessage::Change(change) = message else {
            return self.cache.clear();
        };
        // Rows of a type: every view reading the type, as a write FraiseQL ran would sweep
        // it, but not the levels indexed by collection: `c` names those precisely.
        let views: BTreeSet<&ViewName> = change
            .rows
            .keys()
            .chain(&change.types)
            .filter_map(|type_name| self.type_views.get(type_name))
            .flatten()
            .collect();
        if !views.is_empty() {
            let views: Vec<ViewName> = views.into_iter().cloned().collect();
            self.cache.invalidate_views_except_collections(&views)?;
        }
        for (collection, parents) in &change.collections {
            for parent in parents {
                self.cache.invalidate_collection(collection, parent)?;
            }
        }
        // A type whose rows are not listed may have joined or left any collection of it.
        for type_name in &change.types {
            self.cache.invalidate_collections_of(type_name)?;
        }
        Ok(())
    }
}

/// What a server's listener on [`CHANNEL`] does to its cache (#1589).
pub struct TviewsListener {
    evictor:     TviewsEvictor,
    tagging:     CollectionTagging,
    collections: CollectionMap,
}

impl TviewsListener {
    /// A listener for `cache`, whose composed reads `tagging` indexes, against `catalog`.
    #[must_use]
    pub fn new(
        cache: Arc<QueryResultCache>,
        tagging: CollectionTagging,
        catalog: FeedCatalog,
    ) -> Self {
        Self {
            evictor: TviewsEvictor::new(cache, catalog.type_views),
            tagging,
            collections: catalog.collections,
        }
    }

    /// The session listens on [`CHANNEL`]. Nothing written before is heard: clear the
    /// cache, then index composed reads by collection, in that order (a fill that read the
    /// map is never older than the clear).
    pub fn listening(&self) {
        let _ = self.evictor.cache.clear();
        self.tagging.hear(self.collections.clone());
    }

    /// The session is lost: what is written from now on is not heard. Stop indexing by
    /// collection, then clear the cache, whose fence refuses every fill already in flight.
    pub fn lost(&self) {
        self.tagging.deafen();
        let _ = self.evictor.cache.clear();
    }

    /// Apply `payload`, received on [`CHANNEL`]. An eviction that fails clears the cache:
    /// evicting less than the write requires is a stale answer.
    pub fn receive(&self, payload: &str) {
        if self.evictor.apply(&decode(payload)).is_err() {
            let _ = self.evictor.cache.clear();
        }
    }
}

#[cfg(test)]
#[path = "tviews_feed_tests.rs"]
mod tviews_feed_tests;
