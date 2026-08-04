//! Policy evaluation: the exhaustive matrix, and the parse rules (#371).
//!
//! The matrix is generated, not hand-listed: every (rule-principal × method ×
//! caller-shape) combination is enumerated and checked against an independent
//! statement of the intent, so a future change to `permits` cannot quietly
//! flip a cell that nobody wrote a test for.

#![allow(clippy::unwrap_used)] // Reason: test module

use super::{BucketPolicy, PolicyMethod, PolicyPrincipal, PolicyRequest, PolicyRule};

const ALL_METHODS: [PolicyMethod; 5] = [
    PolicyMethod::Read,
    PolicyMethod::Write,
    PolicyMethod::Overwrite,
    PolicyMethod::Delete,
    PolicyMethod::List,
];

/// The caller shapes that matter to a principal decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Caller {
    Anonymous,
    /// Authenticated, not the owner, no roles.
    Stranger,
    /// Authenticated and the object's owner.
    Owner,
    /// Authenticated, not the owner, carries `auditor`.
    Auditor,
}

const ALL_CALLERS: [Caller; 4] = [
    Caller::Anonymous,
    Caller::Stranger,
    Caller::Owner,
    Caller::Auditor,
];

fn request_for(caller: Caller, key: &str) -> (Option<String>, Vec<String>, Option<String>) {
    let owner = Some("owner-1".to_string());
    let _ = key;
    match caller {
        Caller::Anonymous => (None, vec![], owner),
        Caller::Stranger => (Some("stranger".to_string()), vec![], owner),
        Caller::Owner => (Some("owner-1".to_string()), vec![], owner),
        Caller::Auditor => (Some("stranger".to_string()), vec!["auditor".to_string()], owner),
    }
}

/// Independent statement of what each principal means — deliberately written
/// as a separate expression of intent, not by calling the code under test.
fn principal_should_match(principal: &PolicyPrincipal, caller: Caller) -> bool {
    match principal {
        PolicyPrincipal::Owner => caller == Caller::Owner,
        PolicyPrincipal::Authenticated => caller != Caller::Anonymous,
        PolicyPrincipal::Anonymous => true,
        PolicyPrincipal::Role(role) => role == "auditor" && caller == Caller::Auditor,
    }
}

/// Every (principal × granted-method × method × caller) combination: the rule
/// permits exactly when the method is the granted one AND the principal
/// matches. Everything else denies.
#[test]
fn evaluation_matrix_is_exhaustive_and_denies_by_default() {
    let principals = [
        PolicyPrincipal::Owner,
        PolicyPrincipal::Authenticated,
        PolicyPrincipal::Anonymous,
        PolicyPrincipal::Role("auditor".to_string()),
    ];

    let mut checked = 0_usize;
    for principal in &principals {
        for granted in ALL_METHODS {
            let policy = BucketPolicy {
                rules: vec![PolicyRule {
                    methods:    vec![granted],
                    principal:  principal.clone(),
                    key_prefix: None,
                }],
            };
            for method in ALL_METHODS {
                for caller in ALL_CALLERS {
                    let (user, roles, owner) = request_for(caller, "any/key.txt");
                    let request = PolicyRequest {
                        user_id:  user.as_deref(),
                        roles:    &roles,
                        key:      "any/key.txt",
                        owner_id: owner.as_deref(),
                    };
                    let expected = method == granted && principal_should_match(principal, caller);
                    assert_eq!(
                        policy.permits(method, &request),
                        expected,
                        "principal {principal:?} granted {granted:?}: {method:?} by {caller:?}"
                    );
                    checked += 1;
                }
            }
        }
    }
    assert_eq!(checked, 4 * 5 * 5 * 4, "the matrix must be complete");
}

/// An empty policy — and a policy whose every rule is for other methods —
/// permits nothing at all, for anyone.
#[test]
fn empty_policy_permits_nothing() {
    let empty = BucketPolicy { rules: vec![] };
    let read_only = BucketPolicy {
        rules: vec![PolicyRule {
            methods:    vec![PolicyMethod::Read],
            principal:  PolicyPrincipal::Anonymous,
            key_prefix: None,
        }],
    };
    for caller in ALL_CALLERS {
        let (user, roles, owner) = request_for(caller, "k");
        let request = PolicyRequest {
            user_id:  user.as_deref(),
            roles:    &roles,
            key:      "k",
            owner_id: owner.as_deref(),
        };
        for method in ALL_METHODS {
            assert!(!empty.permits(method, &request), "empty policy denies {method:?}");
        }
        for method in [
            PolicyMethod::Write,
            PolicyMethod::Overwrite,
            PolicyMethod::Delete,
            PolicyMethod::List,
        ] {
            assert!(!read_only.permits(method, &request), "a read rule must not permit {method:?}");
        }
    }
}

/// A `key_prefix` narrows a rule to matching keys and denies elsewhere —
/// including the near-miss that shares a prefix of the prefix.
#[test]
fn key_prefix_narrows_the_rule() {
    let policy = BucketPolicy {
        rules: vec![PolicyRule {
            methods:    vec![PolicyMethod::Read],
            principal:  PolicyPrincipal::Anonymous,
            key_prefix: Some("public/".to_string()),
        }],
    };
    let roles: Vec<String> = vec![];
    let permitted = ["public/a.txt", "public/nested/b.txt"];
    let denied = [
        "private/a.txt",
        "public",
        "publicx/a.txt",
        "a/public/b.txt",
        "",
    ];

    for key in permitted {
        let request = PolicyRequest {
            user_id: None,
            roles: &roles,
            key,
            owner_id: None,
        };
        assert!(policy.permits(PolicyMethod::Read, &request), "{key} is inside the prefix");
    }
    for key in denied {
        let request = PolicyRequest {
            user_id: None,
            roles: &roles,
            key,
            owner_id: None,
        };
        assert!(!policy.permits(PolicyMethod::Read, &request), "{key} is outside the prefix");
    }
}

/// `owner` never matches an object with no owner, and never an unauthenticated
/// caller — the two shapes an "owner" comparison can silently pass on if it
/// compares `Option`s directly.
#[test]
fn owner_principal_never_matches_a_missing_side() {
    let policy = BucketPolicy {
        rules: vec![PolicyRule {
            methods:    vec![PolicyMethod::Read],
            principal:  PolicyPrincipal::Owner,
            key_prefix: None,
        }],
    };
    let roles: Vec<String> = vec![];
    let cases = [
        (None, None, "anonymous caller, ownerless object"),
        (None, Some("owner-1"), "anonymous caller, owned object"),
        (Some("owner-1"), None, "authenticated caller, ownerless object"),
    ];
    for (user_id, owner_id, description) in cases {
        let request = PolicyRequest {
            user_id,
            roles: &roles,
            key: "k",
            owner_id,
        };
        assert!(
            !policy.permits(PolicyMethod::Read, &request),
            "owner must not match: {description}"
        );
    }
}

/// Later rules add permissions; no rule can take one away (there is no deny
/// rule — a policy is a union of permits).
#[test]
fn rules_are_a_union_of_permits() {
    let policy = BucketPolicy {
        rules: vec![
            PolicyRule {
                methods:    vec![PolicyMethod::Read],
                principal:  PolicyPrincipal::Role("auditor".to_string()),
                key_prefix: None,
            },
            PolicyRule {
                methods:    vec![PolicyMethod::Read, PolicyMethod::Delete],
                principal:  PolicyPrincipal::Owner,
                key_prefix: None,
            },
        ],
    };
    let auditor_roles = vec!["auditor".to_string()];
    let auditor = PolicyRequest {
        user_id:  Some("stranger"),
        roles:    &auditor_roles,
        key:      "k",
        owner_id: Some("owner-1"),
    };
    let no_roles: Vec<String> = vec![];
    let owner = PolicyRequest {
        user_id:  Some("owner-1"),
        roles:    &no_roles,
        key:      "k",
        owner_id: Some("owner-1"),
    };

    assert!(policy.permits(PolicyMethod::Read, &auditor));
    assert!(
        !policy.permits(PolicyMethod::Delete, &auditor),
        "the auditor rule grants read only"
    );
    assert!(policy.permits(PolicyMethod::Read, &owner));
    assert!(policy.permits(PolicyMethod::Delete, &owner));
}

#[test]
fn method_parsing_accepts_the_vocabulary_and_rejects_everything_else() {
    assert_eq!(PolicyMethod::parse("read").unwrap(), PolicyMethod::Read);
    assert_eq!(PolicyMethod::parse("WRITE").unwrap(), PolicyMethod::Write);
    assert_eq!(PolicyMethod::parse("overwrite").unwrap(), PolicyMethod::Overwrite);
    assert_eq!(PolicyMethod::parse("Delete").unwrap(), PolicyMethod::Delete);
    assert_eq!(PolicyMethod::parse("list").unwrap(), PolicyMethod::List);
    for bad in ["", "reads", "get", "read ", "*", "rw"] {
        let err = PolicyMethod::parse(bad).expect_err("must be refused");
        assert!(err.contains("unknown policy method"), "{bad}: {err}");
    }
}

#[test]
fn principal_parsing_accepts_the_vocabulary_and_rejects_everything_else() {
    assert_eq!(PolicyPrincipal::parse("owner").unwrap(), PolicyPrincipal::Owner);
    assert_eq!(PolicyPrincipal::parse("Authenticated").unwrap(), PolicyPrincipal::Authenticated);
    assert_eq!(PolicyPrincipal::parse("anonymous").unwrap(), PolicyPrincipal::Anonymous);
    assert_eq!(
        PolicyPrincipal::parse("role:auditor").unwrap(),
        PolicyPrincipal::Role("auditor".to_string())
    );
    // A role name is case-sensitive: roles come from the token verbatim.
    assert_eq!(
        PolicyPrincipal::parse("role:Auditor").unwrap(),
        PolicyPrincipal::Role("Auditor".to_string())
    );
    for bad in [
        "",
        "roles:auditor",
        "role:",
        "role:   ",
        "everyone",
        "user",
        "admin",
    ] {
        assert!(PolicyPrincipal::parse(bad).is_err(), "{bad} must be refused");
    }
}
