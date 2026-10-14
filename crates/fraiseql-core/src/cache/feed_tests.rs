#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

use std::sync::{Arc, Mutex};

use serde_json::json;

use super::*;
use crate::{
    cache::{CacheConfig, CascadeInvalidator, QueryResultCache},
    db::types::JsonbValue,
};

fn views(names: &[&str]) -> Eviction {
    Eviction::Views(names.iter().map(ToString::to_string).collect())
}

fn entity(entity_type: &str, entity_id: &str) -> Eviction {
    Eviction::Entity {
        entity_type: entity_type.to_string(),
        entity_id:   entity_id.to_string(),
    }
}

/// Every eviction `payloads` carry, in one list, checking each came from `from`.
fn decoded(from: &str, payloads: &[String]) -> Vec<Eviction> {
    payloads
        .iter()
        .flat_map(|p| match decode(p) {
            Received::Evictions {
                from: sender,
                evictions,
            } => {
                assert_eq!(sender, from);
                evictions
            },
            Received::Hello { .. } => panic!("not a hello: {p}"),
        })
        .collect()
}

#[test]
fn a_batch_travels_as_one_message_and_reads_back() {
    let payloads = encode("a", [views(&["v_user"]), entity("User", "u1"), views(&["v_post"])]);
    assert_eq!(payloads.len(), 1, "{payloads:?}");
    assert_eq!(
        decoded("a", &payloads),
        vec![views(&["v_post", "v_user"]), entity("User", "u1")]
    );
}

#[test]
fn a_view_named_twice_is_sent_once() {
    let payloads = encode("a", [views(&["v_user"]), views(&["v_user"])]);
    assert_eq!(decoded("a", &payloads), vec![views(&["v_user"])]);
}

#[test]
fn an_empty_batch_sends_nothing() {
    assert!(encode("a", []).is_empty());
}

#[test]
fn evicting_everything_makes_the_batch_one_message() {
    let payloads = encode(
        "a",
        [
            views(&["v_user"]),
            Eviction::Everything,
            entity("User", "u1"),
        ],
    );
    assert_eq!(decoded("a", &payloads), vec![Eviction::Everything]);
}

#[test]
fn a_batch_too_long_for_one_notify_is_split_and_loses_nothing() {
    let names: Vec<String> = (0..1000).map(|i| format!("v_view_number_{i:04}")).collect();
    let ids: Vec<Eviction> = (0..300).map(|i| entity("Order", &format!("{i:036}"))).collect();
    let payloads =
        encode("a", std::iter::once(Eviction::Views(names.clone())).chain(ids.iter().cloned()));

    assert!(payloads.len() > 1, "{} payloads", payloads.len());
    assert!(payloads.iter().all(|p| p.len() <= MAX_PAYLOAD_BYTES), "a payload is too long");
    let mut got_views = Vec::new();
    let mut got_ids = Vec::new();
    for eviction in decoded("a", &payloads) {
        match eviction {
            Eviction::Views(v) => got_views.extend(v),
            e @ Eviction::Entity { .. } => got_ids.push(e),
            Eviction::Everything => panic!("nothing in this batch evicts everything"),
        }
    }
    got_views.sort();
    assert_eq!(got_views, names);
    assert_eq!(got_ids, ids);
}

#[test]
fn a_name_no_notify_can_carry_evicts_everything() {
    let payloads = encode("a", [views(&["v_user"]), entity("User", &"x".repeat(8000))]);
    assert_eq!(decoded("a", &payloads), vec![Eviction::Everything]);
}

#[test]
fn an_unreadable_payload_evicts_everything() {
    for payload in [
        "",
        "not json",
        r#"{"v":1}"#,
        r#"{"v":2,"from":"b","views":["v_user"]}"#,
    ] {
        assert_eq!(
            decode(payload),
            Received::Evictions {
                from:      String::new(),
                evictions: vec![Eviction::Everything],
            },
            "{payload}"
        );
    }
}

#[test]
fn a_hello_reads_back_and_evicts_nothing() {
    assert_eq!(
        decode(&encode_hello("a")),
        Received::Hello {
            from: "a".to_string(),
        }
    );
}

#[tokio::test]
async fn an_announcement_the_queue_cannot_hold_is_recorded() {
    let (outbox, mut queue) = outbox(1);
    outbox.announce(views(&["v_user"]));
    assert!(!queue.take_overflow());
    outbox.announce(views(&["v_post"]));
    assert!(queue.take_overflow(), "the second did not fit");
    assert!(!queue.take_overflow(), "taken once");
    assert_eq!(queue.next_batch().await.unwrap(), vec![views(&["v_user"])]);
}

#[tokio::test]
async fn a_batch_takes_everything_already_queued() {
    let (outbox, mut queue) = outbox(8);
    outbox.announce(views(&["v_user"]));
    outbox.announce(entity("User", "u1"));
    assert_eq!(
        queue.next_batch().await.unwrap(),
        vec![views(&["v_user"]), entity("User", "u1")]
    );
}

fn row(id: &str) -> Vec<JsonbValue> {
    vec![JsonbValue::new(json!({ "id": id }))]
}

fn cache() -> Arc<QueryResultCache> {
    Arc::new(QueryResultCache::new(CacheConfig::enabled()))
}

fn cached(cache: &QueryResultCache, key: u64) -> bool {
    cache.get(key).unwrap().is_some()
}

#[test]
fn an_evictor_evicts_views_through_its_own_embedding_graph() {
    let cache = cache();
    let mut graph = CascadeInvalidator::new();
    graph.add_dependency("tv_post", "tv_user").unwrap();
    let evictor = CacheEvictor {
        cache:   Arc::clone(&cache),
        cascade: Some(Arc::new(Mutex::new(graph))),
    };
    cache.put(1, row("p1"), vec!["tv_post".to_string()], None, None, None).unwrap();
    cache.put(2, row("t1"), vec!["v_tag".to_string()], None, None, None).unwrap();

    evictor.apply(&views(&["tv_user"])).unwrap();

    assert!(!cached(&cache, 1), "tv_post embeds tv_user");
    assert!(cached(&cache, 2), "v_tag reads neither");
}

#[test]
fn an_evictor_evicts_an_entity_and_everything() {
    let cache = cache();
    let evictor = CacheEvictor {
        cache:   Arc::clone(&cache),
        cascade: None,
    };
    let id = "6c9f9c1e-1d5e-4f0e-9b5a-2d1f0c3e4b5a";
    cache
        .put(1, row(id), vec!["v_user".to_string()], None, Some("User"), None)
        .unwrap();
    cache.put(2, row("t1"), vec!["v_tag".to_string()], None, None, None).unwrap();

    evictor.apply(&entity("User", id)).unwrap();
    assert!(!cached(&cache, 1));
    assert!(cached(&cache, 2));

    evictor.apply(&Eviction::Everything).unwrap();
    assert!(!cached(&cache, 2));
}

#[test]
fn a_suspended_cache_is_emptied_and_stores_nothing() {
    let cache = cache();
    cache.put(1, row("a"), vec!["v_user".to_string()], None, None, None).unwrap();

    cache.suspend();
    assert!(!cached(&cache, 1), "suspending clears");
    cache.put(2, row("b"), vec!["v_user".to_string()], None, None, None).unwrap();
    assert!(!cached(&cache, 2), "a suspended cache stores nothing");

    cache.resume();
    cache.put(3, row("c"), vec!["v_user".to_string()], None, None, None).unwrap();
    assert!(cached(&cache, 3), "a resumed cache stores again");
}

#[test]
fn a_read_begun_while_suspended_is_not_stored_after_resuming() {
    let cache = cache();
    cache.suspend();
    let snapshot = cache.invalidation_generation();
    cache.resume();
    cache
        .put(1, row("a"), vec!["v_user".to_string()], None, None, Some(snapshot))
        .unwrap();
    assert!(!cached(&cache, 1), "its rows may predate a write this server never heard of");
}
