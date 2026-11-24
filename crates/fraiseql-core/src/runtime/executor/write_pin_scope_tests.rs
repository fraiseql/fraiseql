//! A request's reads and writes run in its principal's read-your-writes scope,
//! so with read replicas one principal's write pins only its own reads to the primary.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

use std::{collections::HashMap, sync::Arc};

use serde_json::json;

use crate::{
    runtime::{
        Executor,
        executor::test_support::{CapturingMockAdapter, mock_user_results},
    },
    schema::CompiledSchema,
    security::SecurityContext,
};

fn schema() -> CompiledSchema {
    let doc = json!({
        "types": [{"name": "Order", "sql_source": "v_order",
                   "fields": [{"name": "id", "field_type": "ID", "nullable": false}]}],
        "queries": [{"name": "orders", "return_type": "Order", "returns_list": true,
                     "nullable": false, "sql_source": "v_order", "arguments": []}]
    });
    CompiledSchema::from_json(&doc.to_string(), false).expect("fixture schema loads")
}

fn caller(user: &str, tenant: Option<&str>) -> SecurityContext {
    SecurityContext {
        user_id:          user.into(),
        roles:            vec![],
        tenant_id:        tenant.map(Into::into),
        scopes:           vec![],
        attributes:       HashMap::default(),
        request_id:       "req-1".to_string(),
        ip_address:       None,
        expires_at:       fraiseql_time::Timestamp::now()
            + fraiseql_time::jiff::SignedDuration::from_hours(1),
        authenticated_at: fraiseql_time::Timestamp::now(),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    }
}

/// The principal each request's read ran under, one request per entry of `callers`.
async fn principals(callers: &[Option<SecurityContext>]) -> Vec<Option<u64>> {
    let adapter = Arc::new(CapturingMockAdapter::new(mock_user_results()).without_composed_reads());
    let executor = Executor::new(schema(), adapter.clone());
    for caller in callers {
        match caller {
            Some(context) => {
                executor.execute_with_security("{ orders { id } }", None, context).await
            },
            None => executor.execute("{ orders { id } }", None).await,
        }
        .unwrap();
    }
    let seen = adapter.pin_principals.lock().unwrap().clone();
    assert_eq!(seen.len(), callers.len(), "one read per request: {seen:?}");
    seen
}

#[tokio::test]
async fn a_request_reads_in_its_principals_scope() {
    let seen = principals(&[
        Some(caller("alice", Some("acme"))),
        Some(caller("alice", Some("acme"))),
        Some(caller("bob", Some("acme"))),
        Some(caller("alice", Some("globex"))),
    ])
    .await;
    assert!(seen[0].is_some(), "a request with a principal runs in its scope");
    assert_eq!(seen[0], seen[1], "one subject in one tenant is one principal");
    assert_ne!(seen[0], seen[2], "another subject is another principal");
    assert_ne!(seen[0], seen[3], "the same subject in another tenant is another principal");
}

/// A request with no principal runs in no scope, where every write pins its reads.
#[tokio::test]
async fn a_request_with_no_principal_reads_in_no_scope() {
    assert_eq!(principals(&[None]).await, vec![None]);
}
