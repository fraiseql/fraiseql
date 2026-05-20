use std::sync::Arc;

use super::*;

// ── Cycle 2 tests — account linking ───────────────────────────────────

#[tokio::test]
async fn test_first_sign_in_creates_new_account() {
    let store = InMemoryAccountStore::new();
    let result = store
        .link_or_create_user("alice@example.com", "github", "github_123")
        .await
        .unwrap();

    assert!(result.is_new, "first sign-in should create a new account");
    assert!(!result.linked, "no linking on brand-new account");
    assert!(result.user_id.starts_with("user_"), "user_id should have 'user_' prefix");
    assert_eq!(store.len(), 1);
}

#[tokio::test]
async fn test_github_then_google_same_email_returns_same_user_id() {
    // This is the primary Cycle 2 acceptance test.
    let store = InMemoryAccountStore::new();

    // Step 1: user signs in with GitHub
    let github_result = store
        .link_or_create_user("alice@example.com", "github", "github_123")
        .await
        .unwrap();
    assert!(github_result.is_new);
    let user_id = github_result.user_id.clone();

    // Step 2: same user signs in with Google (same email)
    let google_result = store
        .link_or_create_user("alice@example.com", "google", "google_456")
        .await
        .unwrap();
    assert!(!google_result.is_new, "second sign-in should not create a new account");
    assert!(google_result.linked, "Google should be linked to existing account");
    assert_eq!(
        google_result.user_id, user_id,
        "GitHub and Google sign-ins with same email must yield same user_id"
    );

    // Verify only one account record was created
    assert_eq!(store.len(), 1);
}

#[tokio::test]
async fn test_different_emails_create_different_accounts() {
    let store = InMemoryAccountStore::new();

    let alice = store
        .link_or_create_user("alice@example.com", "github", "github_alice")
        .await
        .unwrap();
    let bob = store
        .link_or_create_user("bob@example.com", "github", "github_bob")
        .await
        .unwrap();

    assert_ne!(alice.user_id, bob.user_id, "different emails must produce different user_ids");
    assert_eq!(store.len(), 2);
}

#[tokio::test]
async fn test_same_provider_twice_does_not_duplicate_link() {
    let store = InMemoryAccountStore::new();

    // First sign-in
    store
        .link_or_create_user("alice@example.com", "github", "github_123")
        .await
        .unwrap();

    // Same provider + same provider_id — should NOT add a duplicate link
    let second = store
        .link_or_create_user("alice@example.com", "github", "github_123")
        .await
        .unwrap();
    assert!(!second.is_new, "should not create a new account on second sign-in");
    assert!(!second.linked, "same provider/id should not count as newly linked");

    let record = store.get_account(&second.user_id).await.unwrap();
    assert_eq!(record.providers.len(), 1, "should still have only one provider link");
}

#[tokio::test]
async fn test_multiple_providers_linked_to_single_account() {
    let store = InMemoryAccountStore::new();

    store
        .link_or_create_user("alice@example.com", "github", "github_123")
        .await
        .unwrap();
    store
        .link_or_create_user("alice@example.com", "google", "google_456")
        .await
        .unwrap();
    store
        .link_or_create_user("alice@example.com", "okta", "okta_789")
        .await
        .unwrap();

    let result = store
        .link_or_create_user("alice@example.com", "github", "github_123")
        .await
        .unwrap();
    let record = store.get_account(&result.user_id).await.unwrap();

    assert_eq!(record.providers.len(), 3, "all three providers should be linked");
    let providers: Vec<&str> = record.providers.iter().map(|p| p.provider.as_str()).collect();
    assert!(providers.contains(&"github"));
    assert!(providers.contains(&"google"));
    assert!(providers.contains(&"okta"));
}

#[tokio::test]
async fn test_get_account_returns_correct_record() {
    let store = InMemoryAccountStore::new();
    let result = store
        .link_or_create_user("alice@example.com", "github", "github_123")
        .await
        .unwrap();

    let record = store.get_account(&result.user_id).await.unwrap();
    assert_eq!(record.email, "alice@example.com");
    assert_eq!(record.providers.len(), 1);
    assert_eq!(record.providers[0].provider, "github");
}

#[tokio::test]
async fn test_get_account_unknown_user_id_returns_error() {
    let store = InMemoryAccountStore::new();
    let err = store.get_account("user_nonexistent").await.unwrap_err();
    assert!(
        matches!(err, AuthError::TokenNotFound),
        "unknown user_id should return TokenNotFound, got: {err:?}"
    );
}

#[test]
fn test_normalize_email_lowercases() {
    assert_eq!(normalize_email("Alice@Example.COM"), "alice@example.com");
}

#[test]
fn test_normalize_email_trims_whitespace() {
    assert_eq!(normalize_email("  alice@example.com  "), "alice@example.com");
}

#[test]
fn test_normalize_email_idempotent() {
    let email = "alice@example.com";
    assert_eq!(normalize_email(email), normalize_email(&normalize_email(email)));
}

#[tokio::test]
async fn test_account_store_as_trait_object() {
    let store: Arc<dyn AccountStore> = Arc::new(InMemoryAccountStore::new());
    let result = store
        .link_or_create_user("alice@example.com", "github", "github_123")
        .await
        .unwrap();
    assert!(result.is_new);
}
