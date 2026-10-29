//! No hit is older than an invalidation that completed before it began, under threads
//! (#1778).
//!
//! Writers commit a version to a stand-in database, invalidate, then publish the version
//! as completed. Readers note the completed version before each `get`: a hit holding an
//! older one was served after its invalidation finished. On a miss a reader fills as the
//! executor does: snapshot, read the database, `put` under the snapshot.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    thread,
};

use fraiseql_db::ViewName;
use serde_json::json;

use super::QueryResultCache;
use crate::{cache::CacheConfig, db::types::JsonbValue};

const KEY: u64 = 7;
const WRITERS: usize = 2;
const READERS: usize = 6;
const WRITES: u64 = 2_000;
/// Reads between two writes of one writer: enough for fills to land and be hit between
/// invalidations, and for the race to run while a writer is mid-invalidation.
const PACE: u64 = 64;
/// Hits at the least, so the hit path is what was exercised.
const HITS: u64 = 1_000;

fn hammer(invalidate: fn(&QueryResultCache)) {
    let cache = Arc::new(QueryResultCache::new(CacheConfig::enabled()));
    let database = Arc::new(AtomicU64::new(0));
    let completed = Arc::new(AtomicU64::new(0));
    let writing = Arc::new(AtomicU64::new(WRITERS as u64));
    let reads = Arc::new(AtomicU64::new(0));

    let writers: Vec<_> = (0..WRITERS)
        .map(|_| {
            let (cache, database, completed, writing, reads) = (
                cache.clone(),
                database.clone(),
                completed.clone(),
                writing.clone(),
                reads.clone(),
            );
            thread::spawn(move || {
                for _ in 0..WRITES {
                    let version = database.fetch_add(1, Ordering::SeqCst) + 1;
                    invalidate(&cache);
                    completed.fetch_max(version, Ordering::SeqCst);
                    let mark = reads.load(Ordering::Relaxed) + PACE;
                    while reads.load(Ordering::Relaxed) < mark {
                        thread::yield_now();
                    }
                }
                writing.fetch_sub(1, Ordering::SeqCst);
            })
        })
        .collect();

    // Reason: collected so every reader is spawned now, racing the writers; joined lazily
    // they would start only once the writers had finished.
    #[allow(clippy::needless_collect)]
    let readers: Vec<_> = (0..READERS)
        .map(|_| {
            let (cache, database, completed, writing, reads) = (
                cache.clone(),
                database.clone(),
                completed.clone(),
                writing.clone(),
                reads.clone(),
            );
            thread::spawn(move || {
                let (mut stale, mut hits) = (0_u64, 0_u64);
                while writing.load(Ordering::SeqCst) > 0 {
                    reads.fetch_add(1, Ordering::Relaxed);
                    let floor = completed.load(Ordering::SeqCst);
                    if let Some(rows) = cache.get(KEY).unwrap() {
                        hits += 1;
                        let served = rows[0].as_value()["v"].as_u64().unwrap();
                        if served < floor {
                            stale += 1;
                        }
                    } else {
                        let snapshot = cache.invalidation_generation();
                        let version = database.load(Ordering::SeqCst);
                        let rows = vec![JsonbValue::new(json!({"id": "a", "v": version}))];
                        cache
                            .put(
                                KEY,
                                rows,
                                vec!["tv_user".to_string()],
                                None,
                                Some("User"),
                                Some(snapshot),
                            )
                            .unwrap();
                    }
                }
                (stale, hits)
            })
        })
        .collect();

    for writer in writers {
        writer.join().unwrap();
    }
    let (stale, hits) = readers
        .into_iter()
        .map(|reader| reader.join().unwrap())
        .fold((0, 0), |(stale, hits), (s, h)| (stale + s, hits + h));
    assert!(hits >= HITS, "the race must exercise the hit path: {hits} hits");
    assert_eq!(stale, 0, "hits served after the invalidation of their rows completed");
}

#[test]
fn no_hit_outlives_a_sweep_of_its_view_under_threads() {
    hammer(|cache| cache.invalidate_views(&[ViewName::from("tv_user")]).unwrap());
}

#[test]
fn no_hit_outlives_an_eviction_of_its_row_under_threads() {
    hammer(|cache| {
        cache.invalidate_by_entity("User", "a").unwrap();
    });
}
