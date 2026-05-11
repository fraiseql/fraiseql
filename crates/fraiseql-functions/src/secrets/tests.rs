#![allow(clippy::unwrap_used)] // Reason: test code

use super::*;

fn store() -> InMemorySecretsStore {
    InMemorySecretsStore::new()
}

#[tokio::test]
async fn test_set_and_get_secret() {
    let s = store();
    s.set_secret("my_fn", "API_KEY", "super_secret").await.unwrap();
    let val = s.get_secret("my_fn", "API_KEY").await.unwrap();
    assert_eq!(val, Some("super_secret".to_string()));
}

#[tokio::test]
async fn test_get_missing_secret_returns_none() {
    let s = store();
    let val = s.get_secret("my_fn", "MISSING").await.unwrap();
    assert!(val.is_none());
}

#[tokio::test]
async fn test_set_overwrites_existing_secret() {
    let s = store();
    s.set_secret("fn", "KEY", "v1").await.unwrap();
    s.set_secret("fn", "KEY", "v2").await.unwrap();
    let val = s.get_secret("fn", "KEY").await.unwrap();
    assert_eq!(val, Some("v2".to_string()));
}

#[tokio::test]
async fn test_delete_secret_returns_true_when_found() {
    let s = store();
    s.set_secret("fn", "KEY", "value").await.unwrap();
    let deleted = s.delete_secret("fn", "KEY").await.unwrap();
    assert!(deleted);
    let val = s.get_secret("fn", "KEY").await.unwrap();
    assert!(val.is_none());
}

#[tokio::test]
async fn test_delete_secret_returns_false_when_not_found() {
    let s = store();
    let deleted = s.delete_secret("fn", "GHOST").await.unwrap();
    assert!(!deleted);
}

#[tokio::test]
async fn test_list_secret_keys_returns_names_only() {
    let s = store();
    s.set_secret("fn", "KEY_A", "val_a").await.unwrap();
    s.set_secret("fn", "KEY_B", "val_b").await.unwrap();
    s.set_secret("other_fn", "KEY_X", "val_x").await.unwrap();

    let keys = s.list_secret_keys("fn").await.unwrap();
    assert_eq!(keys, vec!["KEY_A", "KEY_B"]);
}

#[tokio::test]
async fn test_list_secret_keys_empty_when_none_set() {
    let s = store();
    let keys = s.list_secret_keys("fn").await.unwrap();
    assert!(keys.is_empty());
}

#[tokio::test]
async fn test_secrets_scoped_per_function() {
    let s = store();
    s.set_secret("fn_a", "KEY", "value_a").await.unwrap();
    s.set_secret("fn_b", "KEY", "value_b").await.unwrap();

    assert_eq!(s.get_secret("fn_a", "KEY").await.unwrap(), Some("value_a".to_string()));
    assert_eq!(s.get_secret("fn_b", "KEY").await.unwrap(), Some("value_b".to_string()));
}

#[cfg(feature = "function-secrets")]
#[tokio::test]
async fn test_ciphertext_differs_on_each_write() {

    let s = InMemorySecretsStore::new();
    s.set_secret("fn", "KEY", "plaintext").await.unwrap();
    let ct1 = {
        let map = s.store.lock().unwrap();
        map[&("fn".to_string(), "KEY".to_string())].clone()
    };

    // Overwrite with the same value
    s.set_secret("fn", "KEY", "plaintext").await.unwrap();
    let ct2 = {
        let map = s.store.lock().unwrap();
        map[&("fn".to_string(), "KEY".to_string())].clone()
    };

    // Different nonce → different ciphertext even for identical plaintext
    assert_ne!(ct1, ct2, "ciphertext should differ due to random nonce");
}
