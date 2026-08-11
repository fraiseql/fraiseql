//! Policy evaluation: the exhaustive matrix, and the parse rules (#371).
//!
//! The matrix is generated, not hand-listed: every (rule-principal × method ×
//! caller-shape) combination is enumerated and checked against an independent
//! statement of the intent, so a future change to `permits` cannot quietly
//! flip a cell that nobody wrote a test for.

#![allow(clippy::unwrap_used)] // Reason: test module

use chrono::{DateTime, Utc};

use super::{
    BucketPolicy, ClaimValues, PolicyMethod, PolicyPrincipal, PolicyRequest, PolicyRule,
    normalise_claims,
};

/// The claim set for the cases whose decision does not turn on claims.
static NO_CLAIMS: ClaimValues = ClaimValues::new();

/// A rule permitting `methods` to `principal` with no conditions — the #371
/// shape, which every condition below narrows from.
fn permit(methods: Vec<PolicyMethod>, principal: PolicyPrincipal) -> PolicyRule {
    PolicyRule {
        methods,
        principal,
        key_prefix: None,
        not_before: None,
        not_after: None,
        require_unexpired: false,
        require_claims: ClaimValues::new(),
    }
}

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
    /// Anonymous, arriving through the signed-URL path (#974).
    SignedUrlAnonymous,
    /// The owner, arriving through the signed-URL path (#974) — present so the
    /// matrix distinguishes "signed URL" from "no identity".
    SignedUrlOwner,
}

const ALL_CALLERS: [Caller; 6] = [
    Caller::Anonymous,
    Caller::Stranger,
    Caller::Owner,
    Caller::Auditor,
    Caller::SignedUrlAnonymous,
    Caller::SignedUrlOwner,
];

/// `(user_id, roles, owner_id, via_signed_url)` for a caller shape.
fn request_for(caller: Caller, key: &str) -> (Option<String>, Vec<String>, Option<String>, bool) {
    let owner = Some("owner-1".to_string());
    let _ = key;
    match caller {
        Caller::Anonymous => (None, vec![], owner, false),
        Caller::Stranger => (Some("stranger".to_string()), vec![], owner, false),
        Caller::Owner => (Some("owner-1".to_string()), vec![], owner, false),
        Caller::Auditor => {
            (Some("stranger".to_string()), vec!["auditor".to_string()], owner, false)
        },
        Caller::SignedUrlAnonymous => (None, vec![], owner, true),
        Caller::SignedUrlOwner => (Some("owner-1".to_string()), vec![], owner, true),
    }
}

/// Independent statement of what each principal means — deliberately written
/// as a separate expression of intent, not by calling the code under test.
fn principal_should_match(principal: &PolicyPrincipal, caller: Caller) -> bool {
    let via_signed_url = matches!(caller, Caller::SignedUrlAnonymous | Caller::SignedUrlOwner);
    let is_owner = matches!(caller, Caller::Owner | Caller::SignedUrlOwner);
    let is_authenticated = !matches!(caller, Caller::Anonymous | Caller::SignedUrlAnonymous);
    match principal {
        PolicyPrincipal::Owner => is_owner,
        PolicyPrincipal::Authenticated => is_authenticated,
        PolicyPrincipal::Anonymous => true,
        PolicyPrincipal::Role(role) => role == "auditor" && caller == Caller::Auditor,
        // A signed URL is a bearer grant: it says nothing about who presents it.
        PolicyPrincipal::SignedUrl => via_signed_url,
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
        PolicyPrincipal::SignedUrl,
    ];

    let mut checked = 0_usize;
    for principal in &principals {
        for granted in ALL_METHODS {
            let policy = BucketPolicy {
                rules: vec![permit(vec![granted], principal.clone())],
            };
            for method in ALL_METHODS {
                for caller in ALL_CALLERS {
                    let (user, roles, owner, via_signed_url) = request_for(caller, "any/key.txt");
                    let request = PolicyRequest {
                        user_id: user.as_deref(),
                        roles: &roles,
                        key: "any/key.txt",
                        owner_id: owner.as_deref(),
                        now: Utc::now(),
                        expires_at: None,
                        claims: &NO_CLAIMS,
                        via_signed_url,
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
    assert_eq!(
        checked,
        principals.len() * ALL_METHODS.len() * ALL_METHODS.len() * ALL_CALLERS.len(),
        "the matrix must be complete"
    );
}

/// An empty policy — and a policy whose every rule is for other methods —
/// permits nothing at all, for anyone.
#[test]
fn empty_policy_permits_nothing() {
    let empty = BucketPolicy { rules: vec![] };
    let read_only = BucketPolicy {
        rules: vec![PolicyRule {
            methods:           vec![PolicyMethod::Read],
            principal:         PolicyPrincipal::Anonymous,
            key_prefix:        None,
            not_before:        None,
            not_after:         None,
            require_unexpired: false,
            require_claims:    ClaimValues::new(),
        }],
    };
    for caller in ALL_CALLERS {
        let (user, roles, owner, via_signed_url) = request_for(caller, "k");
        let request = PolicyRequest {
            user_id: user.as_deref(),
            roles: &roles,
            key: "k",
            owner_id: owner.as_deref(),
            now: Utc::now(),
            expires_at: None,
            claims: &NO_CLAIMS,
            via_signed_url,
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
            methods:           vec![PolicyMethod::Read],
            principal:         PolicyPrincipal::Anonymous,
            key_prefix:        Some("public/".to_string()),
            not_before:        None,
            not_after:         None,
            require_unexpired: false,
            require_claims:    ClaimValues::new(),
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
            now: Utc::now(),
            expires_at: None,
            claims: &NO_CLAIMS,
            via_signed_url: false,
        };
        assert!(policy.permits(PolicyMethod::Read, &request), "{key} is inside the prefix");
    }
    for key in denied {
        let request = PolicyRequest {
            user_id: None,
            roles: &roles,
            key,
            owner_id: None,
            now: Utc::now(),
            expires_at: None,
            claims: &NO_CLAIMS,
            via_signed_url: false,
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
            methods:           vec![PolicyMethod::Read],
            principal:         PolicyPrincipal::Owner,
            key_prefix:        None,
            not_before:        None,
            not_after:         None,
            require_unexpired: false,
            require_claims:    ClaimValues::new(),
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
            now: Utc::now(),
            expires_at: None,
            claims: &NO_CLAIMS,
            via_signed_url: false,
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
                methods:           vec![PolicyMethod::Read],
                principal:         PolicyPrincipal::Role("auditor".to_string()),
                key_prefix:        None,
                not_before:        None,
                not_after:         None,
                require_unexpired: false,
                require_claims:    ClaimValues::new(),
            },
            PolicyRule {
                methods:           vec![PolicyMethod::Read, PolicyMethod::Delete],
                principal:         PolicyPrincipal::Owner,
                key_prefix:        None,
                not_before:        None,
                not_after:         None,
                require_unexpired: false,
                require_claims:    ClaimValues::new(),
            },
        ],
    };
    let auditor_roles = vec!["auditor".to_string()];
    let auditor = PolicyRequest {
        user_id:        Some("stranger"),
        roles:          &auditor_roles,
        key:            "k",
        owner_id:       Some("owner-1"),
        now:            Utc::now(),
        expires_at:     None,
        claims:         &NO_CLAIMS,
        via_signed_url: false,
    };
    let no_roles: Vec<String> = vec![];
    let owner = PolicyRequest {
        user_id:        Some("owner-1"),
        roles:          &no_roles,
        key:            "k",
        owner_id:       Some("owner-1"),
        now:            Utc::now(),
        expires_at:     None,
        claims:         &NO_CLAIMS,
        via_signed_url: false,
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

// ── #974: closed condition fields ───────────────────────────────────────────

/// A request at `now` about an object expiring at `expires_at`.
fn request_at(
    now: DateTime<Utc>,
    expires_at: Option<DateTime<Utc>>,
    claims: &ClaimValues,
) -> PolicyRequest<'_> {
    PolicyRequest {
        user_id: Some("owner-1"),
        roles: &[],
        key: "k",
        owner_id: Some("owner-1"),
        now,
        expires_at,
        claims,
        via_signed_url: false,
    }
}

fn at(secs: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(1_800_000_000 + secs, 0).expect("valid timestamp")
}

/// `not_before` is inclusive at its own instant and denies strictly before it.
#[test]
fn not_before_denies_until_the_instant_it_names() {
    let mut rule = permit(vec![PolicyMethod::Read], PolicyPrincipal::Owner);
    rule.not_before = Some(at(100));
    let policy = BucketPolicy { rules: vec![rule] };

    for (offset, expected) in [(-1_i64, false), (0, true), (1, true)] {
        assert_eq!(
            policy.permits(PolicyMethod::Read, &request_at(at(100 + offset), None, &NO_CLAIMS)),
            expected,
            "not_before at +{offset}s"
        );
    }
}

/// `not_after` is exclusive: at the instant itself the grant has already ended.
#[test]
fn not_after_is_exclusive_at_its_own_instant() {
    let mut rule = permit(vec![PolicyMethod::Read], PolicyPrincipal::Owner);
    rule.not_after = Some(at(100));
    let policy = BucketPolicy { rules: vec![rule] };

    for (offset, expected) in [(-1_i64, true), (0, false), (1, false)] {
        assert_eq!(
            policy.permits(PolicyMethod::Read, &request_at(at(100 + offset), None, &NO_CLAIMS)),
            expected,
            "not_after at +{offset}s"
        );
    }
}

/// Two adjacent windows must neither overlap nor leave a gap — the property the
/// half-open reading exists for. At the boundary exactly one of them permits.
#[test]
fn adjacent_windows_neither_overlap_nor_gap() {
    let mut first = permit(vec![PolicyMethod::Read], PolicyPrincipal::Owner);
    first.not_after = Some(at(100));
    let mut second = permit(vec![PolicyMethod::Read], PolicyPrincipal::Owner);
    second.not_before = Some(at(100));

    let early = BucketPolicy { rules: vec![first] };
    let late = BucketPolicy {
        rules: vec![second],
    };

    for offset in [-1_i64, 0, 1] {
        let request = request_at(at(100 + offset), None, &NO_CLAIMS);
        let a = early.permits(PolicyMethod::Read, &request);
        let b = late.permits(PolicyMethod::Read, &request);
        assert!(a ^ b, "exactly one window must permit at +{offset}s (got {a} and {b})");
    }
}

/// `require_unexpired` mirrors `now < object.expires_at`, including its answer
/// against a missing expiry: not true, therefore a denial.
#[test]
fn require_unexpired_denies_a_missing_or_past_expiry() {
    let mut rule = permit(vec![PolicyMethod::Read], PolicyPrincipal::Owner);
    rule.require_unexpired = true;
    let policy = BucketPolicy { rules: vec![rule] };

    let cases = [
        (None, false, "an object with no expiry does not satisfy now < expires_at"),
        (Some(at(-1)), false, "an expiry in the past"),
        (Some(at(0)), false, "an expiry exactly now is not in the future"),
        (Some(at(1)), true, "an expiry in the future"),
    ];
    for (expires_at, expected, why) in cases {
        assert_eq!(
            policy.permits(PolicyMethod::Read, &request_at(at(0), expires_at, &NO_CLAIMS)),
            expected,
            "{why}"
        );
    }
}

/// Every entry in `require_claims` must match; a missing or differing claim is a
/// denial, and an empty requirement constrains nothing.
#[test]
fn require_claims_needs_every_claim_to_match_exactly() {
    let mut rule = permit(vec![PolicyMethod::Read], PolicyPrincipal::Owner);
    rule.require_claims = [
        ("tenant".to_string(), "acme".to_string()),
        ("tier".to_string(), "gold".to_string()),
    ]
    .into_iter()
    .collect();
    let policy = BucketPolicy { rules: vec![rule] };

    let cases: [(Vec<(&str, &str)>, bool, &str); 6] = [
        (vec![], false, "no claims at all"),
        (vec![("tenant", "acme")], false, "only one of the two required claims"),
        (vec![("tenant", "acme"), ("tier", "silver")], false, "a differing value"),
        (vec![("tenant", "ACME"), ("tier", "gold")], false, "equality is case-sensitive"),
        (vec![("tenant", "acme"), ("tier", "gold")], true, "both claims match"),
        (
            vec![("tenant", "acme"), ("tier", "gold"), ("extra", "ignored")],
            true,
            "an unrequired extra claim is irrelevant",
        ),
    ];
    for (claims, expected, why) in cases {
        let claims: ClaimValues =
            claims.into_iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        assert_eq!(
            policy.permits(PolicyMethod::Read, &request_at(at(0), None, &claims)),
            expected,
            "{why}"
        );
    }

    let unconstrained = BucketPolicy {
        rules: vec![permit(vec![PolicyMethod::Read], PolicyPrincipal::Owner)],
    };
    assert!(
        unconstrained.permits(PolicyMethod::Read, &request_at(at(0), None, &NO_CLAIMS)),
        "an empty require_claims must constrain nothing"
    );
}

/// The claim normalisation is closed: scalars are matchable, containers are
/// dropped rather than given some invented equality.
#[test]
fn claim_normalisation_keeps_scalars_and_drops_containers() {
    let raw: std::collections::HashMap<String, serde_json::Value> = [
        ("s".to_string(), serde_json::json!("text")),
        ("n".to_string(), serde_json::json!(42)),
        ("b".to_string(), serde_json::json!(true)),
        ("null".to_string(), serde_json::Value::Null),
        ("arr".to_string(), serde_json::json!(["a"])),
        ("obj".to_string(), serde_json::json!({"k": "v"})),
    ]
    .into_iter()
    .collect();

    let claims = normalise_claims(&raw);

    assert_eq!(claims.get("s").map(String::as_str), Some("text"));
    assert_eq!(claims.get("n").map(String::as_str), Some("42"));
    assert_eq!(claims.get("b").map(String::as_str), Some("true"));
    for dropped in ["null", "arr", "obj"] {
        assert!(!claims.contains_key(dropped), "{dropped} must not be matchable");
    }

    // And a rule requiring a dropped claim denies, rather than matching some
    // rendering of it.
    let mut rule = permit(vec![PolicyMethod::Read], PolicyPrincipal::Owner);
    rule.require_claims = std::iter::once(("arr".to_string(), "[\"a\"]".to_string())).collect();
    let policy = BucketPolicy { rules: vec![rule] };
    assert!(!policy.permits(PolicyMethod::Read, &request_at(at(0), None, &claims)));
}

/// The load-bearing property: a condition can only ever remove permissions.
/// Whatever a conditioned rule permits, the same rule without its conditions
/// permits too.
#[test]
fn conditions_only_ever_narrow() {
    let claims: ClaimValues = std::iter::once(("tenant".to_string(), "acme".to_string())).collect();
    let conditioned = {
        let mut rule = permit(ALL_METHODS.to_vec(), PolicyPrincipal::Owner);
        rule.not_before = Some(at(-10));
        rule.not_after = Some(at(10));
        rule.require_unexpired = true;
        rule.require_claims = claims.clone();
        BucketPolicy { rules: vec![rule] }
    };
    let unconditioned = BucketPolicy {
        rules: vec![permit(ALL_METHODS.to_vec(), PolicyPrincipal::Owner)],
    };

    let mut narrowed = 0_usize;
    for offset in [-20_i64, -10, 0, 10, 20] {
        for expiry in [None, Some(at(-1)), Some(at(50))] {
            for claim_set in [&NO_CLAIMS, &claims] {
                for method in ALL_METHODS {
                    let request = request_at(at(offset), expiry, claim_set);
                    let with = conditioned.permits(method, &request);
                    let without = unconditioned.permits(method, &request);
                    assert!(
                        !with || without,
                        "a condition widened access at offset {offset} ({method:?})"
                    );
                    if !with && without {
                        narrowed += 1;
                    }
                }
            }
        }
    }
    assert!(narrowed > 0, "the conditions must actually narrow something");
}

/// A rule skipped for failing a condition must not suppress a later rule that
/// permits — conditions narrow the RULE, not the policy.
#[test]
fn a_failed_condition_does_not_suppress_a_later_rule() {
    let expired_grant = {
        let mut rule = permit(vec![PolicyMethod::Read], PolicyPrincipal::Owner);
        rule.not_after = Some(at(-1));
        rule
    };
    let standing_grant = permit(vec![PolicyMethod::Read], PolicyPrincipal::Owner);
    let policy = BucketPolicy {
        rules: vec![expired_grant, standing_grant],
    };

    assert!(
        policy.permits(PolicyMethod::Read, &request_at(at(0), None, &NO_CLAIMS)),
        "the expired first rule must not shadow the standing second one"
    );
}

/// `signed_url` is parsed, and an unknown principal still names what it expected.
#[test]
fn signed_url_principal_parses_and_unknown_still_errors() {
    assert_eq!(PolicyPrincipal::parse("signed_url").unwrap(), PolicyPrincipal::SignedUrl);
    assert_eq!(PolicyPrincipal::parse("  SIGNED_URL  ").unwrap(), PolicyPrincipal::SignedUrl);

    let err = PolicyPrincipal::parse("signedurl").unwrap_err();
    assert!(err.contains("signed_url"), "the error must list the accepted spelling: {err}");
}
