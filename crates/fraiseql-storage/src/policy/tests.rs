//! Policy evaluation: the exhaustive matrix, and the parse rules (#371).
//!
//! The matrix is generated, not hand-listed: every (rule-principal × method ×
//! caller-shape) combination is enumerated and checked against an independent
//! statement of the intent, so a future change to `permits` cannot quietly
//! flip a cell that nobody wrote a test for.

#![allow(clippy::unwrap_used)] // Reason: test module

use chrono::{DateTime, Utc};

use super::{
    BucketPolicy, ClaimValues, MetadataValues, PolicyMethod, PolicyPrincipal, PolicyRequest,
    PolicyRule, normalise_claims,
};

/// The metadata a request that is not about an object with metadata compares
/// against.
static NO_METADATA: MetadataValues = MetadataValues::new();

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
        require_metadata: MetadataValues::new(),
    }
}

const ALL_METHODS: [PolicyMethod; 6] = [
    PolicyMethod::Read,
    PolicyMethod::Write,
    PolicyMethod::Overwrite,
    PolicyMethod::Delete,
    PolicyMethod::List,
    PolicyMethod::SetMetadata,
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
                        metadata: &NO_METADATA,
                        may_write_metadata: false,
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
            require_metadata:  MetadataValues::new(),
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
            metadata: &NO_METADATA,
            may_write_metadata: false,
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
            require_metadata:  MetadataValues::new(),
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
            metadata: &NO_METADATA,
            may_write_metadata: false,
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
            metadata: &NO_METADATA,
            may_write_metadata: false,
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
            require_metadata:  MetadataValues::new(),
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
            metadata: &NO_METADATA,
            may_write_metadata: false,
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
                require_metadata:  MetadataValues::new(),
            },
            PolicyRule {
                methods:           vec![PolicyMethod::Read, PolicyMethod::Delete],
                principal:         PolicyPrincipal::Owner,
                key_prefix:        None,
                not_before:        None,
                not_after:         None,
                require_unexpired: false,
                require_claims:    ClaimValues::new(),
                require_metadata:  MetadataValues::new(),
            },
        ],
    };
    let auditor_roles = vec!["auditor".to_string()];
    let auditor = PolicyRequest {
        user_id:            Some("stranger"),
        roles:              &auditor_roles,
        key:                "k",
        owner_id:           Some("owner-1"),
        now:                Utc::now(),
        expires_at:         None,
        claims:             &NO_CLAIMS,
        via_signed_url:     false,
        metadata:           &NO_METADATA,
        may_write_metadata: false,
    };
    let no_roles: Vec<String> = vec![];
    let owner = PolicyRequest {
        user_id:            Some("owner-1"),
        roles:              &no_roles,
        key:                "k",
        owner_id:           Some("owner-1"),
        now:                Utc::now(),
        expires_at:         None,
        claims:             &NO_CLAIMS,
        via_signed_url:     false,
        metadata:           &NO_METADATA,
        may_write_metadata: false,
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
        metadata: &NO_METADATA,
        may_write_metadata: false,
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

// ---------------------------------------------------------------------------
// #974: the wire form, and the single parse both doors go through
// ---------------------------------------------------------------------------

mod spec {
    use std::collections::BTreeMap;

    use super::super::{
        BucketPolicy, PolicyMethod, PolicyPrincipal, PolicyRule, PolicyRuleSpec, parse_policy,
        policy_to_specs,
    };

    fn spec(methods: &[&str], principal: &str) -> PolicyRuleSpec {
        PolicyRuleSpec {
            methods:           methods.iter().map(|m| (*m).to_string()).collect(),
            principal:         principal.to_string(),
            key_prefix:        None,
            not_before:        None,
            not_after:         None,
            require_unexpired: false,
            require_claims:    BTreeMap::new(),
            require_metadata:  BTreeMap::new(),
        }
    }

    /// #1099: `require_metadata` is answered by asking whether the caller may
    /// SET metadata — another decision by this same policy. A rule that grants
    /// `set_metadata` and carries `require_metadata` would decide itself, so it
    /// is refused at the door rather than guarded at evaluation time. A runtime
    /// guard would be behaviour an operator cannot read off their own config.
    #[test]
    fn a_rule_that_grants_set_metadata_and_requires_metadata_is_refused() {
        let mut rule = spec(&["set_metadata"], "authenticated");
        rule.require_metadata.insert("classification".to_string(), "public".to_string());
        let err = parse_policy(&[rule]).expect_err("a self-deciding rule must not parse");
        assert!(format!("{err:?}").contains("decide itself"), "{err:?}");
    }

    /// Only the combination is refused. Each half alone is an ordinary rule,
    /// and splitting them into two rules is the documented way to express the
    /// intent.
    #[test]
    fn either_half_alone_parses() {
        parse_policy(&[spec(&["set_metadata"], "authenticated")])
            .expect("a plain set_metadata grant is ordinary");

        let mut gated = spec(&["read"], "authenticated");
        gated
            .require_metadata
            .insert("classification".to_string(), "public".to_string());
        parse_policy(&[gated]).expect("a metadata-gated read is ordinary");

        let mut split_a = spec(&["read"], "authenticated");
        split_a
            .require_metadata
            .insert("classification".to_string(), "public".to_string());
        parse_policy(&[split_a, spec(&["set_metadata"], "role:curator")])
            .expect("two rules express what one may not");
    }

    /// The metadata boundary limits apply to what a POLICY asks for too, not
    /// only to what an object carries. A rule requiring a key no ingestion path
    /// could ever produce is a rule that can never hold, which is the
    /// always-false condition #1099 was split out of #974 to avoid.
    #[test]
    fn a_require_metadata_key_outside_the_charset_is_refused() {
        let mut rule = spec(&["read"], "authenticated");
        rule.require_metadata.insert("a:b".to_string(), "v".to_string());
        let err = parse_policy(&[rule]).expect_err("an unmatchable key must not parse");
        assert!(format!("{err:?}").contains("outside a-z"), "{err:?}");
    }

    /// Every condition survives a render/parse round trip.
    ///
    /// This is what lets an operator `GET` the policy governing a bucket, edit
    /// one rule, and `PUT` it back without a condition silently changing
    /// meaning — and it is why the admin API can report a config-file policy in
    /// the same vocabulary the config file uses.
    #[test]
    fn policy_round_trips_through_the_spec_form() {
        let mut claims = BTreeMap::new();
        claims.insert("tier".to_string(), "gold".to_string());
        let mut meta = BTreeMap::new();
        meta.insert("classification".to_string(), "public".to_string());
        let original = BucketPolicy {
            rules: vec![
                PolicyRule {
                    methods:           vec![PolicyMethod::Read, PolicyMethod::List],
                    principal:         PolicyPrincipal::Role("auditor".to_string()),
                    key_prefix:        Some("reports/".to_string()),
                    not_before:        Some("2026-01-01T00:00:00Z".parse().unwrap()),
                    not_after:         Some("2027-01-01T00:00:00Z".parse().unwrap()),
                    require_unexpired: true,
                    require_claims:    claims,
                    require_metadata:  meta,
                },
                PolicyRule {
                    methods:           vec![PolicyMethod::Read],
                    principal:         PolicyPrincipal::SignedUrl,
                    key_prefix:        None,
                    not_before:        None,
                    not_after:         None,
                    require_unexpired: false,
                    require_claims:    BTreeMap::new(),
                    require_metadata:  BTreeMap::new(),
                },
            ],
        };

        let reparsed = parse_policy(&policy_to_specs(&original)).unwrap();
        assert_eq!(reparsed, original, "rendering and re-parsing must not change a policy");
    }

    /// A rule carrying a field this build does not know is REFUSED, not
    /// silently stripped of it.
    ///
    /// A dropped `require_unexpired` is a rule that stops narrowing while still
    /// reading as if it does — the exact failure the closed vocabulary exists
    /// to prevent. `deny_unknown_fields` is what makes a typo loud.
    #[test]
    fn an_unknown_rule_field_is_refused() {
        let json = serde_json::json!({
            "methods": ["read"],
            "principal": "owner",
            "require_unexpird": true,
        });
        let err = serde_json::from_value::<PolicyRuleSpec>(json).unwrap_err();
        assert!(
            err.to_string().contains("require_unexpird"),
            "the refusal must name the offending field: {err}"
        );
    }

    /// A refusal says which rule is at fault — an operator pushing ten rules
    /// needs to know which one, and so does the boot error.
    #[test]
    fn a_refusal_names_the_offending_rule() {
        let rules = [
            spec(&["read"], "owner"),
            spec(&["read"], "role:ok"),
            spec(&["reed"], "owner"),
        ];
        let err = parse_policy(&rules).unwrap_err();
        assert_eq!(err.rule_index, Some(2));
        assert!(err.message.contains("reed"), "the message must quote the bad spelling: {err}");
        assert!(
            err.to_string().starts_with("policy rule 2:"),
            "the rendered form leads with the index: {err}"
        );
    }

    /// A malformed time bound is refused rather than dropped: a rule that
    /// silently loses its `not_after` permits forever.
    #[test]
    fn a_malformed_time_bound_is_refused() {
        let mut rule = spec(&["read"], "owner");
        rule.not_after = Some("next tuesday".to_string());
        let err = parse_policy(std::slice::from_ref(&rule)).unwrap_err();
        assert_eq!(err.rule_index, Some(0));
        assert!(err.message.contains("not_after"), "{err}");
    }

    /// A window that can never be open is a configuration mistake, not a rule
    /// that permits nothing.
    #[test]
    fn an_inverted_window_is_refused() {
        let mut rule = spec(&["read"], "owner");
        rule.not_before = Some("2027-01-01T00:00:00Z".to_string());
        rule.not_after = Some("2026-01-01T00:00:00Z".to_string());
        let err = parse_policy(std::slice::from_ref(&rule)).unwrap_err();
        assert!(err.message.contains("never permit"), "{err}");
    }

    /// A rule listing no methods is refused for the same reason.
    #[test]
    fn a_rule_with_no_methods_is_refused() {
        let err = parse_policy(&[spec(&[], "owner")]).unwrap_err();
        assert_eq!(err.rule_index, Some(0));
        assert!(err.message.contains("no methods"), "{err}");
    }

    /// An empty rule LIST, by contrast, is a valid lock-down: it parses, and it
    /// permits nothing. Distinct from having no policy at all, which leaves the
    /// coarse access mode governing.
    #[test]
    fn an_empty_rule_list_is_a_valid_lockdown() {
        let policy = parse_policy(&[]).unwrap();
        assert!(policy.rules.is_empty());
    }
}

// ── #1099: set_metadata is its own permission ───────────────────────────────

/// Every spelling in the vocabulary must survive `parse` -> `as_str` -> `parse` AND
/// the serde derive, which are three different renderings of one name.
/// `rename_all = "lowercase"` silently renders `SetMetadata` as `setmetadata`,
/// a spelling `parse` refuses — so a policy could round-trip through the store
/// into a rule that no longer parses.
#[test]
fn every_method_spelling_round_trips_through_parse_as_str_and_serde() {
    for method in [
        PolicyMethod::Read,
        PolicyMethod::Write,
        PolicyMethod::Overwrite,
        PolicyMethod::Delete,
        PolicyMethod::List,
        PolicyMethod::SetMetadata,
    ] {
        let spelled = method.as_str();
        assert_eq!(PolicyMethod::parse(spelled).unwrap(), method, "as_str -> parse: {spelled}");

        let json = serde_json::to_string(&method).unwrap();
        let bare = json.trim_matches('"');
        assert_eq!(bare, spelled, "serde and as_str must agree on {method:?}");
        assert_eq!(
            PolicyMethod::parse(bare).unwrap(),
            method,
            "a serialised method must parse back: {bare}"
        );
    }
}

// ── #1099: the metadata boundary ────────────────────────────────────────────
mod metadata_limits {
    use super::super::{
        MAX_METADATA_KEY_LEN, MAX_METADATA_KEYS, MAX_METADATA_VALUE_LEN, MetadataValues,
        validate_metadata,
    };

    fn map(pairs: &[(&str, &str)]) -> MetadataValues {
        pairs.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect()
    }

    #[test]
    fn an_ordinary_map_passes_through_unchanged() {
        let input = map(&[("classification", "public"), ("retention-years", "7")]);
        assert_eq!(validate_metadata(&input).unwrap(), input);
    }

    #[test]
    fn keys_are_lower_cased_because_header_names_are_case_insensitive() {
        let got = validate_metadata(&map(&[("Classification", "public")])).unwrap();
        assert_eq!(got.get("classification").map(String::as_str), Some("public"));
        assert!(!got.contains_key("Classification"), "the original casing must not survive");
    }

    /// Not last-one-wins. Two keys that differ only in case collapse to one, and
    /// which value survives would depend on `BTreeMap` iteration order — i.e. on
    /// the keys' bytes. A policy matching on the survivor would be deciding
    /// access on that.
    #[test]
    fn two_keys_differing_only_in_case_are_a_conflict_not_a_silent_overwrite() {
        let err = validate_metadata(&map(&[("Owner", "a"), ("owner", "b")])).unwrap_err();
        assert!(err.contains("differ only in case"), "{err}");
    }

    #[test]
    fn too_many_keys_is_refused_naming_the_count() {
        let pairs: MetadataValues =
            (0..=MAX_METADATA_KEYS).map(|i| (format!("k{i}"), "v".to_string())).collect();
        let err = validate_metadata(&pairs).unwrap_err();
        assert!(err.contains(&MAX_METADATA_KEYS.to_string()), "{err}");
    }

    #[test]
    fn an_oversized_key_is_refused_naming_the_key() {
        let long = "k".repeat(MAX_METADATA_KEY_LEN + 1);
        let err = validate_metadata(&map(&[(long.as_str(), "v")])).unwrap_err();
        assert!(err.contains("bytes"), "{err}");
    }

    #[test]
    fn an_oversized_value_is_refused_naming_the_key() {
        let long = "v".repeat(MAX_METADATA_VALUE_LEN + 1);
        let err = validate_metadata(&map(&[("k", long.as_str())])).unwrap_err();
        assert!(err.contains("\"k\""), "the refusal must name the offending key: {err}");
    }

    #[test]
    fn an_empty_key_is_refused() {
        assert!(validate_metadata(&map(&[("", "v")])).unwrap_err().contains("empty"));
    }

    /// The charset excludes every delimiter that would let a key look
    /// structured. A key is a name, not a path or a namespace — nothing in this
    /// design may come to depend on parsing one.
    #[test]
    fn keys_carrying_a_delimiter_are_refused() {
        for bad in ["a:b", "a/b", "a b", "a=b", "a,b", "a\tb", "a\u{00e9}b"] {
            let err = validate_metadata(&map(&[(bad, "v")]))
                .expect_err("a key carrying a delimiter must be refused");
            assert!(err.contains("outside a-z"), "{bad}: {err}");
        }
    }

    /// The limits are refusals, not truncations. A value silently cut to
    /// 1024 bytes is a value a policy would compare against and never match,
    /// with nothing anywhere saying why.
    #[test]
    fn a_value_at_exactly_the_limit_is_accepted() {
        let at_limit = "v".repeat(MAX_METADATA_VALUE_LEN);
        let got = validate_metadata(&map(&[("k", at_limit.as_str())])).unwrap();
        assert_eq!(got.get("k").map(String::len), Some(MAX_METADATA_VALUE_LEN));
    }

    #[test]
    fn exactly_the_maximum_number_of_keys_is_accepted() {
        let pairs: MetadataValues =
            (0..MAX_METADATA_KEYS).map(|i| (format!("k{i}"), "v".to_string())).collect();
        assert_eq!(validate_metadata(&pairs).unwrap().len(), MAX_METADATA_KEYS);
    }
}

// ── #1099: require_metadata, and what makes it trustworthy ──────────────────
//
// Metadata is caller-writable, so a condition matching on it is in general a
// condition the gated caller controls. What makes this one safe is not a
// reserved key namespace policed at the upload door — it is that writing
// metadata is its own grant, and the condition refuses to hold for anyone who
// holds it.
mod require_metadata {
    use chrono::Utc;

    use super::{
        BucketPolicy, ClaimValues, MetadataValues, NO_CLAIMS, PolicyMethod, PolicyPrincipal,
        PolicyRequest, PolicyRule,
    };

    fn meta(pairs: &[(&str, &str)]) -> MetadataValues {
        pairs.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect()
    }

    /// A `read` rule for any authenticated caller, narrowed by `require_metadata`.
    fn read_requiring(pairs: &[(&str, &str)]) -> BucketPolicy {
        BucketPolicy {
            rules: vec![PolicyRule {
                methods:           vec![PolicyMethod::Read],
                principal:         PolicyPrincipal::Authenticated,
                key_prefix:        None,
                not_before:        None,
                not_after:         None,
                require_unexpired: false,
                require_claims:    ClaimValues::new(),
                require_metadata:  meta(pairs),
            }],
        }
    }

    fn request<'a>(
        metadata: &'a MetadataValues,
        may_write_metadata: bool,
        roles: &'a [String],
    ) -> PolicyRequest<'a> {
        PolicyRequest {
            user_id: Some("user-1"),
            roles,
            key: "k.txt",
            owner_id: Some("someone-else"),
            now: Utc::now(),
            expires_at: None,
            claims: &NO_CLAIMS,
            via_signed_url: false,
            metadata,
            may_write_metadata,
        }
    }

    #[test]
    fn it_holds_when_the_object_carries_the_required_value() {
        let object = meta(&[("classification", "public")]);
        assert!(
            read_requiring(&[("classification", "public")])
                .permits(PolicyMethod::Read, &request(&object, false, &[])),
            "an object carrying the required value must be readable"
        );
    }

    #[test]
    fn it_fails_when_the_value_differs() {
        let object = meta(&[("classification", "secret")]);
        assert!(
            !read_requiring(&[("classification", "public")])
                .permits(PolicyMethod::Read, &request(&object, false, &[]))
        );
    }

    /// Fail-closed on absence, exactly like `require_claims`. An absent key is
    /// not a wildcard — the alternative would make every object created before
    /// the policy existed readable.
    #[test]
    fn an_absent_key_fails_rather_than_matching_anything() {
        let object = MetadataValues::new();
        assert!(
            !read_requiring(&[("classification", "public")])
                .permits(PolicyMethod::Read, &request(&object, false, &[]))
        );
    }

    #[test]
    fn every_required_key_must_match_not_just_one() {
        let object = meta(&[("classification", "public"), ("tier", "bronze")]);
        assert!(
            !read_requiring(&[("classification", "public"), ("tier", "gold")])
                .permits(PolicyMethod::Read, &request(&object, false, &[])),
            "conditions are conjunctive"
        );
    }

    /// **The guarantee.** The object carries exactly what the rule asks for, and
    /// the rule would otherwise permit — but this caller may write the object's
    /// metadata, so the value proves nothing about them. Denying is the only
    /// answer that does not let a caller author the input to their own access
    /// decision.
    #[test]
    fn it_fails_for_a_caller_who_may_write_the_metadata_it_matches_on() {
        let object = meta(&[("classification", "public")]);
        let policy = read_requiring(&[("classification", "public")]);

        assert!(
            policy.permits(PolicyMethod::Read, &request(&object, false, &[])),
            "control: the same request permits when the caller cannot set metadata"
        );
        assert!(
            !policy.permits(PolicyMethod::Read, &request(&object, true, &[])),
            "#1099: a caller who may set this object's metadata cannot satisfy \
             require_metadata with it"
        );
    }

    /// The flag narrows only this condition. A rule that does not use
    /// `require_metadata` is unaffected by who may write metadata — otherwise
    /// granting `set_metadata` would silently revoke unrelated access.
    #[test]
    fn a_rule_without_require_metadata_is_unaffected_by_the_flag() {
        let policy = read_requiring(&[]);
        let object = MetadataValues::new();
        for may_write in [false, true] {
            assert!(
                policy.permits(PolicyMethod::Read, &request(&object, may_write, &[])),
                "an unconditioned rule must not depend on may_write_metadata ({may_write})"
            );
        }
    }
}
