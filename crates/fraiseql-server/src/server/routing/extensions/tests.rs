//! Unit tests for the storage routes' authentication: the static-token helper
//! (`storage_admin_user`) and the caller's admission.

use super::storage_admin_user;

#[test]
fn admin_user_is_none_when_no_token_configured() {
    assert!(
        storage_admin_user("any-presented-token", None).is_none(),
        "no configured storage_token must never grant admin",
    );
}

#[test]
fn admin_user_is_none_when_configured_token_is_empty() {
    // A misconfigured empty `storage_token` must not grant admin to a bare
    // `Authorization: Bearer ` (empty presented token) or anything else.
    assert!(storage_admin_user("", Some("")).is_none());
    assert!(storage_admin_user("whatever", Some("")).is_none());
}

#[test]
fn admin_user_is_some_on_exact_match() {
    let user = storage_admin_user("s3cr3t-admin-token", Some("s3cr3t-admin-token"))
        .expect("an exact token match should yield an admin user");
    // The static-token admin grant must carry the explicit storage-admin role
    // (NOT the generic `"admin"`) so it stays in lockstep with the storage RLS
    // evaluator after the M-storage-scope decollision.
    assert_eq!(
        user.roles,
        vec![fraiseql_storage::STORAGE_ADMIN_ROLE.to_string()],
        "admin user carries the explicit storage-admin role",
    );
    assert_ne!(user.roles, vec!["admin".to_string()], "must NOT carry the generic admin role");
    assert!(user.user_id.is_some(), "admin user has a stable identifier");
}

#[test]
fn admin_user_is_none_on_mismatch() {
    assert!(storage_admin_user("wrong-token", Some("right-token")).is_none());
    // A prefix of the configured token must not match.
    assert!(storage_admin_user("right", Some("right-token")).is_none());
    // A superstring of the configured token must not match.
    assert!(storage_admin_user("right-token-plus", Some("right-token")).is_none());
}

/// A storage request is identified as every transport identifies one: the static
/// `storage_token` is the storage admin, and otherwise a service-account secret, a static
/// API key or a bearer token names the caller, through admission (#1643).
mod storage_admission {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use std::sync::Arc;

    use axum::{body::to_bytes, http::StatusCode};
    use fraiseql_core::{runtime::Executor, schema::CompiledSchema};
    use fraiseql_test_utils::failing_adapter::FailingAdapter;
    use http::HeaderMap;

    use super::super::identify_storage_caller;
    use crate::{
        api_key::static_key_authenticator, routes::graphql::AppState,
        service_account::one_account_authenticator,
    };

    const KEY: &str = "storage-static-key";
    const ACCOUNT_SECRET: &str = "storage-account-secret";
    const STORAGE_TOKEN: &str = "storage-admin-token";

    fn state() -> AppState {
        let executor =
            Arc::new(Executor::new(CompiledSchema::default(), Arc::new(FailingAdapter::new())));
        let mut state = AppState::new(executor)
            .with_api_key_authenticator(Arc::new(static_key_authenticator(KEY)));
        state.service_account_authenticator = Some(one_account_authenticator(ACCOUNT_SECRET));
        state
    }

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for (name, value) in pairs {
            headers.insert(
                http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                value.parse().unwrap(),
            );
        }
        headers
    }

    /// The caller's user id, or the refusal's status and body.
    async fn identify(
        state: &AppState,
        pairs: &[(&str, &str)],
    ) -> Result<Option<String>, (StatusCode, String)> {
        match identify_storage_caller(state, Some(STORAGE_TOKEN), None, &headers(pairs)).await {
            Ok(user) => Ok(user.and_then(|user| user.user_id)),
            Err(response) => {
                let status = response.status();
                let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
                Err((status, String::from_utf8(body.to_vec()).unwrap()))
            },
        }
    }

    #[tokio::test]
    async fn a_static_api_key_identifies_a_storage_caller() {
        let caller = identify(&state(), &[("x-api-key", KEY)]).await.unwrap();
        assert_eq!(caller.as_deref(), Some("apikey:static"));
    }

    #[tokio::test]
    async fn a_service_account_secret_identifies_a_storage_caller() {
        let caller = identify(&state(), &[("x-api-key", ACCOUNT_SECRET)]).await.unwrap();
        assert!(caller.is_some_and(|id| id != "storage-admin"), "the account, not the admin");
    }

    #[tokio::test]
    async fn a_bearer_beside_a_service_account_secret_is_refused_as_ambiguous() {
        let refused = identify(
            &state(),
            &[
                ("authorization", "Bearer any-token"),
                ("x-api-key", ACCOUNT_SECRET),
            ],
        )
        .await
        .unwrap_err();
        assert_eq!(
            refused,
            (
                StatusCode::UNAUTHORIZED,
                crate::service_account::AMBIGUOUS_CREDENTIALS.to_string()
            )
        );
    }

    #[cfg(feature = "auth")]
    #[tokio::test]
    async fn a_subject_enrichment_denies_is_refused() {
        let mut state = state();
        state.identity = crate::identity::RequestIdentity::default()
            .with_resolver(Arc::new(crate::identity::tests::resolver_returning(&[])));
        let refused = identify(&state, &[("x-api-key", KEY)]).await.unwrap_err();
        assert_eq!(refused.0, StatusCode::FORBIDDEN, "{}", refused.1);
    }

    /// A validated bearer token names its subject, and one whose session was revoked is
    /// refused in the words every transport uses.
    #[tokio::test]
    async fn a_bearer_token_is_validated_and_checked_for_revocation() {
        use crate::{
            testing::idp::{TestIdp, token},
            token_revocation::{InMemoryRevocationStore, RevocationStore, TokenRevocationManager},
        };

        let idp = TestIdp::start().await;
        let store = Arc::new(InMemoryRevocationStore::new());
        store.revoke("j-revoked", 3600).await.unwrap();
        let state = state().with_revocation_manager(Arc::new(TokenRevocationManager::new(
            store, false, false, 3600,
        )));
        let validator = idp.validator(true);
        for (jti, expected) in [
            ("j-live", Ok(Some("alice".to_string()))),
            (
                "j-revoked",
                Err((StatusCode::UNAUTHORIZED, crate::admission::TOKEN_REVOKED.to_string())),
            ),
        ] {
            let bearer = format!("Bearer {}", token("alice", jti));
            let headers = headers(&[("authorization", bearer.as_str())]);
            let answer = match identify_storage_caller(
                &state,
                Some(STORAGE_TOKEN),
                Some(&validator),
                &headers,
            )
            .await
            {
                Ok(user) => Ok(user.and_then(|user| user.user_id)),
                Err(response) => {
                    let status = response.status();
                    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
                    Err((status, String::from_utf8(body.to_vec()).unwrap()))
                },
            };
            assert_eq!(answer, expected, "{jti}");
        }
    }

    /// The controls: the storage token is the admin, a request with no credential is
    /// anonymous, and without a validator any other bearer is a rejected admin attempt.
    #[tokio::test]
    async fn the_storage_token_and_anonymous_requests_are_answered_as_before() {
        let state = state();
        let bearer = format!("Bearer {STORAGE_TOKEN}");
        let admin = identify(&state, &[("authorization", bearer.as_str())]).await.unwrap();
        assert_eq!(admin.as_deref(), Some("storage-admin"));
        assert_eq!(identify(&state, &[]).await.unwrap(), None);
        let refused = identify(&state, &[("authorization", "Bearer other")]).await.unwrap_err();
        assert_eq!(refused, (StatusCode::UNAUTHORIZED, "Invalid storage token".to_string()));
    }
}
