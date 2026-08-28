use chrono::Utc;

use super::{STORAGE_ADMIN_ROLE, StorageCaller, StorageRlsEvaluator};
use crate::{
    config::{BucketAccess, BucketConfig},
    metadata::StorageMetadataRow,
    policy::ClaimValues,
};

/// The claim set for callers whose decisions do not turn on claims.
static NO_CLAIMS: ClaimValues = ClaimValues::new();

/// A caller for the tests that predate #974's conditions: no claims, not
/// through a signed URL, decided now.
fn caller<'a>(user_id: Option<&'a str>, roles: &'a [String]) -> StorageCaller<'a> {
    StorageCaller::new(user_id, roles, &NO_CLAIMS, Utc::now())
}

fn private_bucket() -> BucketConfig {
    BucketConfig {
        name: "private-bucket".to_string(),
        max_object_bytes: None,
        allowed_mime_types: None,
        access: BucketAccess::Private,
        transform_presets: None,
        serve_inline: false,
        policies: None,
        upload_ttl_secs: None,
        ..BucketConfig::default()
    }
}

fn public_bucket() -> BucketConfig {
    BucketConfig {
        name: "public-bucket".to_string(),
        max_object_bytes: None,
        allowed_mime_types: None,
        access: BucketAccess::PublicRead,
        transform_presets: None,
        serve_inline: false,
        policies: None,
        upload_ttl_secs: None,
        ..BucketConfig::default()
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
        pending:           false,
        created_at:        Utc::now(),
        updated_at:        Utc::now(),
        expires_at:        None,
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
    assert!(eval.can_read(&caller(Some("user-1"), &user_roles()), &private_bucket(), &obj));
}

#[test]
fn test_rls_denies_non_owner_read_on_private_bucket() {
    let eval = StorageRlsEvaluator::new();
    let obj = object_owned_by("user-1");
    assert!(!eval.can_read(&caller(Some("user-2"), &user_roles()), &private_bucket(), &obj));
}

#[test]
fn test_rls_allows_public_bucket_read() {
    let eval = StorageRlsEvaluator::new();
    let obj = object_owned_by("user-1");
    // Anonymous read on public bucket
    assert!(eval.can_read(&caller(None, &[]), &public_bucket(), &obj));
}

#[test]
fn test_rls_allows_admin_role_bypass() {
    let eval = StorageRlsEvaluator::new();
    let obj = object_owned_by("user-1");
    // Admin can read anyone's objects in private buckets
    assert!(eval.can_read(&caller(Some("admin-user"), &admin_roles()), &private_bucket(), &obj));
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
        !eval.can_read(&caller(Some("attacker"), &generic_admin), &private_bucket(), &obj),
        "generic 'admin' role must not read another user's private object",
    );
    assert!(
        !eval.can_delete(&caller(Some("attacker"), &generic_admin), &private_bucket(), &obj),
        "generic 'admin' role must not delete another user's object",
    );
    assert!(
        !eval.can_write_object(
            &caller(Some("attacker"), &generic_admin),
            &private_bucket(),
            &obj.key,
            Some(&obj)
        ),
        "generic 'admin' role must not overwrite another user's object",
    );

    // The explicit storage-admin role still confers full access (the intended grant).
    assert!(eval.can_read(&caller(Some("ops"), &admin_roles()), &private_bucket(), &obj));
    assert!(eval.can_delete(&caller(Some("ops"), &admin_roles()), &private_bucket(), &obj));
    assert!(eval.can_write_object(
        &caller(Some("ops"), &admin_roles()),
        &private_bucket(),
        &obj.key,
        Some(&obj)
    ));
}

#[test]
fn test_rls_denies_upload_without_permission() {
    let eval = StorageRlsEvaluator::new();
    // Anonymous user cannot write
    assert!(!eval.can_write_key(&caller(None, &[]), &private_bucket(), "f.txt"));
}

#[test]
fn test_rls_allows_authenticated_upload() {
    let eval = StorageRlsEvaluator::new();
    assert!(eval.can_write_key(&caller(Some("user-1"), &user_roles()), &private_bucket(), "f.txt"));
}

#[test]
fn test_rls_denies_delete_by_non_owner() {
    let eval = StorageRlsEvaluator::new();
    let obj = object_owned_by("user-1");
    assert!(!eval.can_delete(&caller(Some("user-2"), &user_roles()), &private_bucket(), &obj));
}

#[test]
fn test_rls_allows_delete_by_owner() {
    let eval = StorageRlsEvaluator::new();
    let obj = object_owned_by("user-1");
    assert!(eval.can_delete(&caller(Some("user-1"), &user_roles()), &private_bucket(), &obj));
}

#[test]
fn test_rls_allows_admin_delete() {
    let eval = StorageRlsEvaluator::new();
    let obj = object_owned_by("user-1");
    assert!(eval.can_delete(&caller(Some("admin-user"), &admin_roles()), &private_bucket(), &obj));
}

// ── can_write_object: create vs overwrite (H9 / B4 overwrite IDOR) ──────────

#[test]
fn test_can_write_object_create_allows_authenticated() {
    let eval = StorageRlsEvaluator::new();
    assert!(eval.can_write_object(
        &caller(Some("user-1"), &user_roles()),
        &private_bucket(),
        "new.txt",
        None
    ));
}

#[test]
fn test_can_write_object_create_denies_anonymous() {
    let eval = StorageRlsEvaluator::new();
    assert!(!eval.can_write_object(&caller(None, &[]), &private_bucket(), "new.txt", None));
}

#[test]
fn test_can_write_object_overwrite_allows_owner() {
    let eval = StorageRlsEvaluator::new();
    let obj = object_owned_by("user-1");
    assert!(eval.can_write_object(
        &caller(Some("user-1"), &user_roles()),
        &private_bucket(),
        &obj.key,
        Some(&obj)
    ));
}

#[test]
fn test_can_write_object_overwrite_denies_non_owner() {
    let eval = StorageRlsEvaluator::new();
    let obj = object_owned_by("user-1");
    assert!(
        !eval.can_write_object(
            &caller(Some("user-2"), &user_roles()),
            &private_bucket(),
            &obj.key,
            Some(&obj)
        ),
        "H9: a non-owner must not overwrite another user's object"
    );
}

#[test]
fn test_can_write_object_overwrite_allows_admin() {
    let eval = StorageRlsEvaluator::new();
    let obj = object_owned_by("user-1");
    assert!(eval.can_write_object(
        &caller(Some("admin-user"), &admin_roles()),
        &private_bucket(),
        &obj.key,
        Some(&obj)
    ));
}

#[test]
fn test_can_write_object_overwrite_denies_anonymous() {
    let eval = StorageRlsEvaluator::new();
    let obj = object_owned_by("user-1");
    assert!(!eval.can_write_object(&caller(None, &[]), &private_bucket(), &obj.key, Some(&obj)));
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
                pending:           false,
                created_at:        Utc::now(),
                updated_at:        Utc::now(),
                expires_at:        None,
            }
        })
        .collect();

    let visible =
        eval.filter_visible(&caller(Some("user-1"), &user_roles()), &private_bucket(), objects);
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
            pending:           false,
            created_at:        Utc::now(),
            updated_at:        Utc::now(),
            expires_at:        None,
        })
        .collect();

    // Anonymous user on public bucket sees everything
    let visible = eval.filter_visible(&caller(None, &[]), &public_bucket(), objects);
    assert_eq!(visible.len(), 5);
}

// ── #1100: a key_prefix-scoped write rule must permit creates under it ───────
//
// `can_write_object`'s CREATE branch decided against an empty key, so
// `"".starts_with("uploads/")` was false and a prefixed `write` rule permitted
// no create anywhere. The motivating shape from #371 — "members may write under
// `uploads/`" — was inexpressible, while the `overwrite` half of the same rule
// honoured the prefix (it is decided against `object.key`). One rule, two
// behaviours.
//
// Fail-closed, so this was a usability/correctness defect rather than a hole —
// and the fix WIDENS a security control, which is why it carries its own matrix
// here and at all three write doors in `routes::tests`.

use crate::policy::{BucketPolicy, PolicyMethod, PolicyPrincipal, PolicyRule};

/// A bucket whose only grant is `write` under `prefix`, to any authenticated
/// caller. No condition narrows it further, so the prefix is the only thing the
/// decision can turn on.
fn bucket_with_write_under(prefix: &str) -> BucketConfig {
    BucketConfig {
        policies: Some(BucketPolicy {
            rules: vec![PolicyRule {
                methods:           vec![PolicyMethod::Write],
                principal:         PolicyPrincipal::Authenticated,
                key_prefix:        Some(prefix.to_string()),
                not_before:        None,
                not_after:         None,
                require_unexpired: false,
                require_claims:    crate::policy::ClaimValues::new(),
            }],
        }),
        ..private_bucket()
    }
}

#[test]
fn a_prefixed_write_rule_permits_a_create_under_its_prefix() {
    let eval = StorageRlsEvaluator::new();
    assert!(
        eval.can_write_object(
            &caller(Some("user-1"), &user_roles()),
            &bucket_with_write_under("uploads/"),
            "uploads/f.txt",
            None,
        ),
        "#1100: `write` under `uploads/` must permit creating `uploads/f.txt`"
    );
}

#[test]
fn a_prefixed_write_rule_still_denies_a_create_outside_its_prefix() {
    let eval = StorageRlsEvaluator::new();
    assert!(
        !eval.can_write_object(
            &caller(Some("user-1"), &user_roles()),
            &bucket_with_write_under("uploads/"),
            "other/f.txt",
            None,
        ),
        "the prefix must still narrow: widening the create path must not widen it past the rule"
    );
}

#[test]
fn a_prefixed_write_rule_denies_an_anonymous_create_under_its_prefix() {
    let eval = StorageRlsEvaluator::new();
    assert!(
        !eval.can_write_object(
            &caller(None, &[]),
            &bucket_with_write_under("uploads/"),
            "uploads/f.txt",
            None
        ),
        "`principal = authenticated` still decides; the key only narrows"
    );
}

#[test]
fn a_prefixed_write_rule_does_not_permit_an_overwrite_under_its_prefix() {
    let eval = StorageRlsEvaluator::new();
    let mut obj = object_owned_by("user-2");
    obj.key = "uploads/f.txt".to_string();
    assert!(
        !eval.can_write_object(
            &caller(Some("user-1"), &user_roles()),
            &bucket_with_write_under("uploads/"),
            "uploads/f.txt",
            Some(&obj),
        ),
        "H9/B4: replacing an EXISTING object is `overwrite`, never `write` — threading the \
         key through the create branch must not let a `write` grant clobber another \
         user's object"
    );
}

#[test]
fn an_unprefixed_write_rule_still_permits_a_create_anywhere() {
    let eval = StorageRlsEvaluator::new();
    let bucket = BucketConfig {
        policies: Some(BucketPolicy {
            rules: vec![PolicyRule {
                methods:           vec![PolicyMethod::Write],
                principal:         PolicyPrincipal::Authenticated,
                key_prefix:        None,
                not_before:        None,
                not_after:         None,
                require_unexpired: false,
                require_claims:    crate::policy::ClaimValues::new(),
            }],
        }),
        ..private_bucket()
    };
    assert!(
        eval.can_write_object(
            &caller(Some("user-1"), &user_roles()),
            &bucket,
            "anywhere/at/all.txt",
            None,
        ),
        "an absent prefix means the whole bucket, as it always did"
    );
}
