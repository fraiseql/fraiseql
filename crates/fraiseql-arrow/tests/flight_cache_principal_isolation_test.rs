//! The Flight query-result cache must not serve one principal's rows to another (#716).
//!
//! `execute_optimized_view` runs its SQL through the raw database adapter, so no
//! per-user row filtering happens on that path — which makes the cache the last
//! place a principal boundary can still be observed. Keying it on the SQL text
//! alone means two principals issuing the identical query share one entry.
//!
//! Driven through `do_get` so the principal under test is the one the live path
//! derives from the session token, not one a test constructed by hand.
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
#![allow(clippy::default_trait_access)] // Reason: test code uses Default::default() for struct fields

use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use arrow_flight::{Ticket, flight_service_server::FlightService as _};
use async_trait::async_trait;
use chrono::Utc;
use fraiseql_arrow::{ArrowDatabaseAdapter, DatabaseResult, FlightTicket, FraiseQLFlightService};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use serde::Serialize;
use tonic::Request;

const TEST_SECRET: &str = "flight-cache-isolation-secret-32";

/// Counts how many reads actually reach the database.
struct CountingAdapter {
    calls: AtomicUsize,
}

#[async_trait]
impl ArrowDatabaseAdapter for CountingAdapter {
    async fn execute_raw_query(
        &self,
        _sql: &str,
    ) -> DatabaseResult<Vec<HashMap<String, serde_json::Value>>> {
        let n = self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(vec![
            [
                ("id".to_string(), serde_json::json!("1")),
                ("email".to_string(), serde_json::json!("a@example.com")),
                ("name".to_string(), serde_json::json!(format!("call-{n}"))),
                ("created_at".to_string(), serde_json::json!("2024-01-01T00:00:00Z")),
            ]
            .into_iter()
            .collect(),
        ])
    }
}

#[derive(Serialize)]
struct TestClaims {
    sub:          String,
    exp:          i64,
    iat:          i64,
    scopes:       Vec<String>,
    session_type: String,
}

fn session_token_for(user: &str) -> String {
    let now = Utc::now();
    let claims = TestClaims {
        sub:          user.to_string(),
        exp:          (now + chrono::Duration::minutes(5)).timestamp(),
        iat:          now.timestamp(),
        scopes:       vec!["user".to_string()],
        session_type: "flight".to_string(),
    };
    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(TEST_SECRET.as_bytes()),
    )
    .expect("token must encode")
}

/// One authenticated `OptimizedView` read of `ta_users` as `user`.
async fn read_as(service: &FraiseQLFlightService, user: &str) {
    let ticket = FlightTicket::OptimizedView {
        view:     "ta_users".to_string(),
        filter:   None,
        order_by: None,
        limit:    None,
        offset:   None,
    };
    let mut req = Request::new(Ticket {
        ticket: ticket.encode().expect("ticket must encode").into(),
    });
    req.metadata_mut().insert(
        "authorization",
        format!("Bearer {}", session_token_for(user))
            .parse()
            .expect("header must parse"),
    );
    service
        .do_get(req)
        .await
        .unwrap_or_else(|e| panic!("do_get as {user} must succeed: {e}"));
}

#[tokio::test]
async fn two_principals_do_not_share_one_flight_cache_entry() {
    let adapter = Arc::new(CountingAdapter {
        calls: AtomicUsize::new(0),
    });
    let service = FraiseQLFlightService::new_with_cache(
        Arc::clone(&adapter) as Arc<dyn ArrowDatabaseAdapter>,
        60,
    )
    .with_session_secret(TEST_SECRET.to_string());

    read_as(&service, "alice").await;
    assert_eq!(adapter.calls.load(Ordering::SeqCst), 1, "alice's read hits the database");

    // A repeat read by the same principal is exactly what the cache is for.
    read_as(&service, "alice").await;
    assert_eq!(
        adapter.calls.load(Ordering::SeqCst),
        1,
        "the cache must still serve a repeat read by the same principal"
    );

    // Bob issues the identical SQL. The cache key must separate him from Alice.
    read_as(&service, "bob").await;
    assert_eq!(
        adapter.calls.load(Ordering::SeqCst),
        2,
        "#716: a second principal issuing the same SQL must not be served the first \
         principal's cached rows"
    );

    // …and Bob's own entry is cached, so the isolation did not simply disable caching.
    read_as(&service, "bob").await;
    assert_eq!(
        adapter.calls.load(Ordering::SeqCst),
        2,
        "per-principal keying must still cache: bob's repeat read must not hit the database"
    );
}
