use chrono::Utc;

use super::{STORAGE_ADMIN_ROLE, StorageRlsEvaluator};
use crate::{
    config::{BucketAccess, BucketConfig},
    metadata::StorageMetadataRow,
};

fn private_bucket() -> BucketConfig {
    BucketConfig {
        name:               "private-bucket".to_string(),
        max_object_bytes:   None,
        allowed_mime_types: None,
        access:             BucketAccess::Private,
        transform_presets:  None,
        serve_inline:       false,
    }
}

fn public_bucket() -> BucketConfig {
    BucketConfig {
        name:               "public-bucket".to_string(),
        max_object_bytes:   None,
        allowed_mime_types: None,
        access:             BucketAccess::PublicRead,
        transform_presets:  None,
        serve_inline:       false,
    }
}

fn object_owned_by(owner: &str) -> StorageMetadataRow {
    StorageMetadataRow {
        pk_storage_object: 1,
        bucket:            "test".to_string(),
        key:               "file.txt".to_string(),
        content_type:      "text/plain".to_string(),
        size_bytes:        100,
        etag:              None,
        owner_id:          Some(owner.to_string()),
        created_at:        Utc::now(),
        updated_at:        Utc::now(),
    }
}

fn admin_roles() -> Vec<String> {
    vec![STORAGE_ADMIN_ROLE.to_string()]
}

fn user_roles() -> Vec<String> {
    vec!["user".to_string()]
}

#[test]
fn test_rls_allows_owner_to_read_own_object() {
    let eval = StorageRlsEvaluator::new();
    let obj = object_owned_by("user-1");
    assert!(eval.can_read(Some("user-1"), &user_roles(), &private_bucket(), &obj));
}

#[test]
fn test_rls_denies_non_owner_read_on_private_bucket() {
    let eval = StorageRlsEvaluator::new();
    let obj = object_owned_by("user-1");
    assert!(!eval.can_read(Some("user-2"), &user_roles(), &private_bucket(), &obj));
}

#[test]
fn test_rls_allows_public_bucket_read() {
    let eval = StorageRlsEvaluator::new();
    let obj = object_owned_by("user-1");
    // Anonymous read on public bucket
    assert!(eval.can_read(None, &[], &public_bucket(), &obj));
}

#[test]
fn test_rls_allows_admin_role_bypass() {
    let eval = StorageRlsEvaluator::new();
    let obj = object_owned_by("user-1");
    // Admin can read anyone's objects in private buckets
    assert!(eval.can_read(Some("admin-user"), &admin_roles(), &private_bucket(), &obj));
}

/// Phase 03 C6 — M-storage-scope: the generic role `"admin"` must NOT confer
/// storage-admin privileges. The server maps an OIDC token's scopes verbatim
/// into a user's storage roles, so a token carrying an unrelated `admin` scope
/// (common in many IdPs/apps) must not be able to read, overwrite, or delete
/// another user's objects. Only the explicit `fraiseql:storage:admin` role does.
#[test]
fn test_rls_generic_admin_role_is_not_storage_admin() {
    let eval = StorageRlsEvaluator::new();
    let obj = object_owned_by("user-1");
    let generic_admin = vec!["admin".to_string()];

    assert!(
        !eval.can_read(Some("attacker"), &generic_admin, &private_bucket(), &obj),
        "generic 'admin' role must not read another user's private object",
    );
    assert!(
        !eval.can_delete(Some("attacker"), &generic_admin, &private_bucket(), &obj),
        "generic 'admin' role must not delete another user's object",
    );
    assert!(
        !eval.can_write_object(Some("attacker"), &generic_admin, &private_bucket(), Some(&obj)),
        "generic 'admin' role must not overwrite another user's object",
    );

    // The explicit storage-admin role still confers full access (the intended grant).
    assert!(eval.can_read(Some("ops"), &admin_roles(), &private_bucket(), &obj));
    assert!(eval.can_delete(Some("ops"), &admin_roles(), &private_bucket(), &obj));
    assert!(eval.can_write_object(Some("ops"), &admin_roles(), &private_bucket(), Some(&obj)));
}

#[test]
fn test_rls_denies_upload_without_permission() {
    let eval = StorageRlsEvaluator::new();
    // Anonymous user cannot write
    assert!(!eval.can_write(None, &[], &private_bucket()));
}

#[test]
fn test_rls_allows_authenticated_upload() {
    let eval = StorageRlsEvaluator::new();
    assert!(eval.can_write(Some("user-1"), &user_roles(), &private_bucket()));
}

#[test]
fn test_rls_denies_delete_by_non_owner() {
    let eval = StorageRlsEvaluator::new();
    let obj = object_owned_by("user-1");
    assert!(!eval.can_delete(Some("user-2"), &user_roles(), &private_bucket(), &obj));
}

#[test]
fn test_rls_allows_delete_by_owner() {
    let eval = StorageRlsEvaluator::new();
    let obj = object_owned_by("user-1");
    assert!(eval.can_delete(Some("user-1"), &user_roles(), &private_bucket(), &obj));
}

#[test]
fn test_rls_allows_admin_delete() {
    let eval = StorageRlsEvaluator::new();
    let obj = object_owned_by("user-1");
    assert!(eval.can_delete(Some("admin-user"), &admin_roles(), &private_bucket(), &obj));
}

// ── can_write_object: create vs overwrite (H9 / B4 overwrite IDOR) ──────────

#[test]
fn test_can_write_object_create_allows_authenticated() {
    let eval = StorageRlsEvaluator::new();
    assert!(eval.can_write_object(Some("user-1"), &user_roles(), &private_bucket(), None));
}

#[test]
fn test_can_write_object_create_denies_anonymous() {
    let eval = StorageRlsEvaluator::new();
    assert!(!eval.can_write_object(None, &[], &private_bucket(), None));
}

#[test]
fn test_can_write_object_overwrite_allows_owner() {
    let eval = StorageRlsEvaluator::new();
    let obj = object_owned_by("user-1");
    assert!(eval.can_write_object(Some("user-1"), &user_roles(), &private_bucket(), Some(&obj)));
}

#[test]
fn test_can_write_object_overwrite_denies_non_owner() {
    let eval = StorageRlsEvaluator::new();
    let obj = object_owned_by("user-1");
    assert!(
        !eval.can_write_object(Some("user-2"), &user_roles(), &private_bucket(), Some(&obj)),
        "H9: a non-owner must not overwrite another user's object"
    );
}

#[test]
fn test_can_write_object_overwrite_allows_admin() {
    let eval = StorageRlsEvaluator::new();
    let obj = object_owned_by("user-1");
    assert!(eval.can_write_object(
        Some("admin-user"),
        &admin_roles(),
        &private_bucket(),
        Some(&obj)
    ));
}

#[test]
fn test_can_write_object_overwrite_denies_anonymous() {
    let eval = StorageRlsEvaluator::new();
    let obj = object_owned_by("user-1");
    assert!(!eval.can_write_object(None, &[], &private_bucket(), Some(&obj)));
}

#[test]
fn test_rls_list_filters_to_visible_objects() {
    let eval = StorageRlsEvaluator::new();

    let objects: Vec<StorageMetadataRow> = (0..5)
        .map(|i| {
            let owner = if i < 3 { "user-1" } else { "user-2" };
            StorageMetadataRow {
                pk_storage_object: i64::from(i),
                bucket:            "private-bucket".to_string(),
                key:               format!("file-{i}.txt"),
                content_type:      "text/plain".to_string(),
                size_bytes:        100,
                etag:              None,
                owner_id:          Some(owner.to_string()),
                created_at:        Utc::now(),
                updated_at:        Utc::now(),
            }
        })
        .collect();

    let visible = eval.filter_visible(Some("user-1"), &user_roles(), &private_bucket(), objects);
    assert_eq!(visible.len(), 3, "user-1 owns 3 of 5 objects");
    assert!(visible.iter().all(|o| o.owner_id.as_deref() == Some("user-1")));
}

#[test]
fn test_rls_list_public_bucket_shows_all() {
    let eval = StorageRlsEvaluator::new();

    let objects: Vec<StorageMetadataRow> = (0..5)
        .map(|i| StorageMetadataRow {
            pk_storage_object: i64::from(i),
            bucket:            "public-bucket".to_string(),
            key:               format!("file-{i}.txt"),
            content_type:      "text/plain".to_string(),
            size_bytes:        100,
            etag:              None,
            owner_id:          Some("someone".to_string()),
            created_at:        Utc::now(),
            updated_at:        Utc::now(),
        })
        .collect();

    // Anonymous user on public bucket sees everything
    let visible = eval.filter_visible(None, &[], &public_bucket(), objects);
    assert_eq!(visible.len(), 5);
}
