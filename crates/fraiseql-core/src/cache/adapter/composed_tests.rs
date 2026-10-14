//! Cached composed reads (#1589).

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::sync::atomic::{AtomicU32, Ordering};

use fraiseql_db::{ComposedLevel, ViewName};
use serde_json::json;

use super::*;
use crate::{
    backend::{ComposedEmbed, EmbedShape, EmbedSource, LevelKeys, ScalarFieldType},
    cache::{CacheConfig, collections::CollectionMap, tviews_feed::Collection},
};

/// Answers every composed read with one post and no comments, counting the reads.
struct ComposedAdapter {
    reads: AtomicU32,
}

// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait::async_trait]
impl DatabaseAdapter for ComposedAdapter {
    fn applies_session_variables(&self) -> bool {
        true
    }

    async fn execute_where_query(
        &self,
        _view: &str,
        _where_clause: Option<&WhereClause>,
        _limit: Option<u32>,
        _offset: Option<u32>,
        _order_by: Option<&[OrderByClause]>,
    ) -> Result<Vec<JsonbValue>> {
        Ok(vec![])
    }

    async fn execute_with_projection(
        &self,
        _view: &str,
        _projection: Option<&crate::schema::SqlProjectionHint>,
        _where_clause: Option<&WhereClause>,
        _limit: Option<u32>,
        _offset: Option<u32>,
        _order_by: Option<&[OrderByClause]>,
    ) -> Result<Vec<JsonbValue>> {
        Ok(vec![])
    }

    async fn execute_composed_with_session(
        &self,
        _read: &ComposedLevel,
        _session_vars: &[(&str, &str)],
        _routing: ReadRouting,
    ) -> Result<Arc<Vec<JsonbValue>>> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        Ok(Arc::new(vec![JsonbValue::new(
            json!({"d": {"id": "p1"}, "e": {"comments": []}}),
        )]))
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::PostgreSQL
    }

    async fn health_check(&self) -> Result<()> {
        Ok(())
    }

    fn pool_metrics(&self) -> PoolMetrics {
        PoolMetrics {
            total_connections:  1,
            idle_connections:   1,
            active_connections: 0,
            waiting_requests:   0,
        }
    }

    async fn execute_raw_query(
        &self,
        _sql: &str,
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
        Ok(vec![])
    }

    async fn execute_parameterized_aggregate(
        &self,
        _sql: &str,
        _params: &[serde_json::Value],
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
        Ok(vec![])
    }
}

fn adapter() -> CachedDatabaseAdapter<ComposedAdapter> {
    CachedDatabaseAdapter::new(
        ComposedAdapter {
            reads: AtomicU32::new(0),
        },
        QueryResultCache::new(CacheConfig::enabled()),
        "v1".to_string(),
    )
}

fn level(view: &str) -> ComposedLevel {
    ComposedLevel {
        view:         view.to_string(),
        projection:   None,
        where_clause: None,
        order_by:     None,
        limit:        Some(20),
        offset:       None,
        keyset:       None,
        keys:         LevelKeys::Whole,
        embeds:       Vec::new(),
    }
}

/// Posts with their comments.
fn posts_with_comments() -> ComposedLevel {
    let mut posts = level("tv_post");
    posts.embeds.push(ComposedEmbed {
        output_key: "comments".to_string(),
        shape:      EmbedShape::Many,
        source:     EmbedSource::Correlated {
            target_key: vec!["post_id".to_string()],
            parent_key: vec!["id".to_string()],
            key_type:   ScalarFieldType::Text,
        },
        level:      level("tv_comment"),
    });
    posts
}

async fn read(adapter: &CachedDatabaseAdapter<ComposedAdapter>, routing: ReadRouting) -> u32 {
    adapter
        .execute_composed_with_session(&posts_with_comments(), &[], routing)
        .await
        .unwrap();
    adapter.inner().reads.load(Ordering::SeqCst)
}

fn hear_comments_of_posts(adapter: &CachedDatabaseAdapter<ComposedAdapter>) {
    adapter.collection_tagging().hear(CollectionMap::from_levels([(
        ("tv_comment", "tv_post"),
        Collection::new("Comment", "fk_post"),
    )]));
}

#[tokio::test]
async fn a_composed_read_is_served_from_the_cache() {
    let adapter = adapter();
    assert_eq!(read(&adapter, ReadRouting::Any).await, 1);
    assert_eq!(read(&adapter, ReadRouting::Any).await, 1, "the second read is a hit");
}

#[tokio::test]
async fn a_read_that_refuses_staleness_is_not_cached() {
    let adapter = adapter();
    assert_eq!(read(&adapter, ReadRouting::Primary).await, 1);
    assert_eq!(read(&adapter, ReadRouting::Primary).await, 2);
}

#[tokio::test]
async fn a_write_to_an_embedded_view_evicts_the_composed_read() {
    let adapter = adapter();
    read(&adapter, ReadRouting::Any).await;
    adapter.invalidate_views(&[ViewName::from("tv_comment")]).unwrap();
    assert_eq!(read(&adapter, ReadRouting::Any).await, 2, "its comments may have changed");
}

#[tokio::test]
async fn while_the_feed_is_heard_a_list_is_evicted_by_its_collection() {
    let adapter = adapter();
    hear_comments_of_posts(&adapter);
    read(&adapter, ReadRouting::Any).await;

    let cache = adapter.cache();
    cache
        .invalidate_views_except_collections(&[ViewName::from("tv_comment")])
        .unwrap();
    assert_eq!(read(&adapter, ReadRouting::Any).await, 1, "a comment of another post");

    cache
        .invalidate_collection(&Collection::new("Comment", "fk_post"), "p1")
        .unwrap();
    assert_eq!(read(&adapter, ReadRouting::Any).await, 2, "a comment of p1");
}

#[tokio::test]
async fn while_the_feed_is_heard_a_write_through_fraiseql_still_evicts_at_once() {
    let adapter = adapter();
    hear_comments_of_posts(&adapter);
    read(&adapter, ReadRouting::Any).await;
    adapter.invalidate_views(&[ViewName::from("tv_comment")]).unwrap();
    assert_eq!(read(&adapter, ReadRouting::Any).await, 2);
}

#[tokio::test]
async fn an_opted_out_root_view_is_not_cached() {
    let adapter = adapter().with_view_ttl_overrides(HashMap::from([("tv_user".to_string(), 60)]));
    assert_eq!(read(&adapter, ReadRouting::Any).await, 1);
    assert_eq!(read(&adapter, ReadRouting::Any).await, 2);
}
