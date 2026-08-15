//! #967: the thread key, which is the whole security question.
#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use axum::http::HeaderMap;
use fraiseql_core::{security::SecurityContext, types::UserId};

use super::{ThreadKey, thread_key};

fn ctx(user_id: &str) -> SecurityContext {
    let mut c = SecurityContext::system_job("t", "r", vec![], vec![], None);
    c.user_id = UserId::new(user_id);
    c
}

fn headers(session: Option<&str>) -> HeaderMap {
    let mut h = HeaderMap::new();
    if let Some(s) = session {
        h.insert("mcp-session-id", s.parse().unwrap());
    }
    h
}

/// The header partitions a principal's own threads.
#[test]
fn one_principal_partitions_its_own_threads_by_the_header() {
    let a = thread_key(Some(&ctx("alice")), &headers(Some("thread-1"))).unwrap();
    let b = thread_key(Some(&ctx("alice")), &headers(Some("thread-2"))).unwrap();

    assert_eq!(a.session_id, b.session_id, "same principal, same session");
    assert_ne!(a.thread_id, b.thread_id, "different header, different thread");
}

/// **The one that matters.** Two principals sending the *same*
/// `mcp-session-id` must not land on the same thread.
///
/// The header is client-controlled, so if it were the key, any caller could read
/// and overwrite any other's durable thread by sending their id. Keying the
/// session on the authenticated `user_id` and the thread on the header is what
/// makes the header a partition **inside** a principal rather than an address
/// across principals.
#[test]
fn two_principals_sending_the_same_header_do_not_share_a_thread() {
    let session = headers(Some("collide"));
    let alice = thread_key(Some(&ctx("alice")), &session).unwrap();
    let mallory = thread_key(Some(&ctx("mallory")), &session).unwrap();

    assert_eq!(alice.thread_id, mallory.thread_id, "they did send the same header");
    assert_ne!(
        alice.session_id, mallory.session_id,
        "…and must still address different stores — the session is derived from the \
         authenticated principal, never from anything the client sends"
    );
}

/// The derivation is stable across processes, so a thread survives a restart.
#[test]
fn the_session_id_is_a_pure_function_of_the_principal() {
    let first = thread_key(Some(&ctx("alice")), &headers(Some("t"))).unwrap();
    let second = thread_key(Some(&ctx("alice")), &headers(Some("t"))).unwrap();
    assert_eq!(first, second);
    assert_eq!(
        first,
        ThreadKey {
            session_id: uuid::Uuid::new_v5(&super::PRINCIPAL_NAMESPACE, b"alice"),
            thread_id:  "t".to_string(),
        }
    );
}

/// No principal, no thread — rather than an unscoped one every anonymous caller
/// would share.
#[test]
fn an_anonymous_caller_gets_no_thread() {
    assert!(thread_key(None, &headers(Some("thread-1"))).is_none());
}

/// No header, no thread: the caller has not asked for one, and inventing an id
/// would silently start writing state nobody requested.
#[test]
fn a_caller_without_the_header_gets_no_thread() {
    assert!(thread_key(Some(&ctx("alice")), &headers(None)).is_none());
    assert!(
        thread_key(Some(&ctx("alice")), &headers(Some("   "))).is_none(),
        "a blank header is no header"
    );
}
