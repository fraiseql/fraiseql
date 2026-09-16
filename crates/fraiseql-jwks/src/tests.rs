//! The bound itself, on both sides of every threshold it has.
//!
//! The consumer-side proof that this is reached lives with the consumers — the
//! `[auth]` path's suite counts the requests *its* validator makes. What is
//! pinned here is the rule: a miss costs one request, concurrent misses cost one
//! between them, an expired set is not served, and an operator's refresh is not
//! subject to either.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use serde_json::json;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

use crate::{Jwk, JwkSet, JwksError, JwksSource, MonotonicClock, REFETCH_COOLDOWN};

/// Where the fixture publisher serves its keys.
const JWKS_PATH: &str = "/.well-known/jwks.json";

/// A TTL long enough that nothing below expires by accident. Expiry is exercised
/// by advancing the clock, never by waiting.
const LONG_TTL: Duration = Duration::from_secs(3600);

/// An RSA modulus of the right shape. The tests here never verify a signature —
/// they count requests and select keys — so only the *selection* has to be real.
const RSA_N: &str = "qX924rB7f6JjpFRR8_8W-KdeJpHjnq4OG2pQDdDc524nlW5pysD42mlH0vWbKcMAp_Wy2yAGjCGtxqCqrEsnyCE1jYpIi6fGZ82kB_CW4Dyxmkn78uMOnU6dHJvZC-LXuikZNie6MG1XacFup3xSsmMLTQCS0g4Oml-xwifUAKFk_a1gvJSrwqHU8GL639k-T5C43vFA2WB8dVgvk_W2pftLipfoT-4-TSDPOCodltCnzBDpDJRnUOq_GO2krrptqmJx-SvIEqFpQU5-9_dxfnW5ExpnxpaPZN-n5ZiIbtXhVlIK2IRuR3SsKHziOB0BB_XLlvkR6Za6bUug_QSZ0Q";

/// A clock the test moves, so both sides of a window are reachable without
/// sleeping through it — and so a test that means to exercise expiry cannot pass
/// merely because the machine was slow.
#[derive(Debug)]
struct TestClock {
    origin: Instant,
    offset: AtomicU64,
}

impl TestClock {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            origin: Instant::now(),
            offset: AtomicU64::new(0),
        })
    }

    fn advance(&self, by: Duration) {
        let millis = u64::try_from(by.as_millis()).expect("a test advances by a sane duration");
        self.offset.fetch_add(millis, Ordering::SeqCst);
    }
}

impl MonotonicClock for TestClock {
    fn now(&self) -> Instant {
        self.origin + Duration::from_millis(self.offset.load(Ordering::SeqCst))
    }
}

/// A publisher serving `kids`, counting every GET, answering after `delay`.
async fn publisher(kids: &[&str], delay: Duration) -> MockServer {
    let mock = MockServer::start().await;
    mount(&mock, kids, delay).await;
    mock
}

async fn mount(mock: &MockServer, kids: &[&str], delay: Duration) {
    let keys: Vec<serde_json::Value> = kids
        .iter()
        .map(|kid| json!({ "kty": "RSA", "kid": kid, "alg": "RS256", "use": "sig", "n": RSA_N, "e": "AQAB" }))
        .collect();
    Mock::given(method("GET"))
        .and(path(JWKS_PATH))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"keys": keys})).set_delay(delay),
        )
        .mount(mock)
        .await;
}

/// How many times the fixture publisher was asked for its keys.
async fn fetches(mock: &MockServer) -> usize {
    mock.received_requests()
        .await
        .expect("the fixture publisher records its requests")
        .len()
}

fn source_for(mock: &MockServer, ttl: Duration, clock: &Arc<TestClock>) -> JwksSource {
    JwksSource::new(&format!("{}{JWKS_PATH}", mock.uri()), ttl)
        .expect("a loopback http jwks_uri is accepted")
        .with_clock(clock.clone())
}

// ============================================================================
// The bound
// ============================================================================

#[tokio::test]
async fn a_miss_is_remembered_until_the_cooldown_lapses() {
    let mock = publisher(&["published"], Duration::ZERO).await;
    let clock = TestClock::new();
    let source = source_for(&mock, LONG_TTL, &clock);

    for index in 0..20 {
        assert!(
            source.key(&format!("unknown-{index}")).await.unwrap().is_none(),
            "a kid the publisher does not serve is None, not an error"
        );
    }
    assert_eq!(
        fetches(&mock).await,
        1,
        "twenty distinct unknown kids inside one cooldown must cost one fetch. A miss that is \
         never remembered lets a caller set this server's outbound request rate"
    );
}

#[tokio::test]
async fn a_key_published_after_the_cooldown_is_found() {
    let mock = publisher(&["old"], Duration::ZERO).await;
    let clock = TestClock::new();
    let source = source_for(&mock, LONG_TTL, &clock);

    assert!(source.key("rotated-in").await.unwrap().is_none(), "not published yet");
    assert_eq!(fetches(&mock).await, 1);

    // The publisher rotates a key in. Nothing about the held set says so.
    mock.reset().await;
    mount(&mock, &["old", "rotated-in"], Duration::ZERO).await;

    // Still inside the cooldown: the new key is not looked for. This is the price
    // of the bound, and it is bounded by the constant rather than open-ended.
    assert!(source.key("rotated-in").await.unwrap().is_none(), "inside the cooldown");
    assert_eq!(fetches(&mock).await, 0, "no request while the cooldown holds");

    clock.advance(REFETCH_COOLDOWN);

    assert!(
        source.key("rotated-in").await.unwrap().is_some(),
        "past the cooldown the publisher is asked again, so a genuine rotation is picked up. A \
         bound that never lets go is an authentication outage of its own"
    );
    assert_eq!(fetches(&mock).await, 1);
}

#[tokio::test]
async fn an_unreachable_publisher_is_asked_once_per_cooldown_too() {
    // The attempt is what cools down, not the success. A publisher that is down
    // otherwise turns every inbound request into an outbound one — the same
    // amplification, reached by a different door, and the door that opens exactly
    // when the `IdP` is already struggling.
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(JWKS_PATH))
        .respond_with(ResponseTemplate::new(503))
        .mount(&mock)
        .await;
    let clock = TestClock::new();
    let source = source_for(&mock, LONG_TTL, &clock);

    let first = source.key("any").await.expect_err("the fetch itself failed");
    assert!(matches!(first, JwksError::Unreachable { .. }), "got {first}");

    for _ in 1..20 {
        let error = source.key("any").await.expect_err(
            "with no key set held at all, `not published` is a claim this source cannot make: \
             a caller mapping it to HTTP would report the operator's own broken key fetch to \
             the sender as a 401 (#1045)",
        );
        assert!(
            matches!(error, JwksError::Unavailable { .. }),
            "inside the cooldown the remembered failure is replayed, not re-attempted: {error}"
        );
        assert!(
            error.to_string().contains("503"),
            "and it still names why, so the operator is not left guessing: {error}"
        );
    }
    assert_eq!(
        fetches(&mock).await,
        1,
        "twenty lookups against a publisher answering 503 must cost one request. Recording the \
         attempt only on success leaves the failure path unbounded"
    );

    clock.advance(REFETCH_COOLDOWN);
    let _ = source.key("any").await;
    assert_eq!(
        fetches(&mock).await,
        2,
        "and the next cooldown allows exactly one more — twenty-one lookups, two requests"
    );
}

#[tokio::test]
async fn a_held_set_that_lacks_the_key_stays_a_plain_miss_when_a_refetch_fails() {
    // The counterweight to the test above. Once a key set IS held, "that kid is
    // not in it" is a real answer and must not be upgraded to an error just
    // because the publisher blipped — or one bad minute at the `IdP` would turn
    // every forged delivery into a 5xx.
    let mock = publisher(&["published"], Duration::ZERO).await;
    let clock = TestClock::new();
    let source = source_for(&mock, LONG_TTL, &clock);

    assert!(source.key("published").await.unwrap().is_some(), "the set is held");

    mock.reset().await;
    Mock::given(method("GET"))
        .and(path(JWKS_PATH))
        .respond_with(ResponseTemplate::new(503))
        .mount(&mock)
        .await;
    clock.advance(REFETCH_COOLDOWN);

    let error = source.key("unknown").await.expect_err("this refetch does fail");
    assert!(matches!(error, JwksError::Unreachable { .. }), "got {error}");

    // Inside the new cooldown the held set answers, and it answers `None`.
    assert!(
        source.key("unknown").await.unwrap().is_none(),
        "a held set that does not name the kid is a miss, not `I cannot say`"
    );
    assert!(
        source.key("published").await.unwrap().is_some(),
        "and the held key still verifies"
    );
}

#[tokio::test]
async fn concurrent_misses_make_one_request_between_them() {
    // The response is delayed so every lookup is in flight before the first
    // finishes. Without the delay a cooldown alone passes this — the fetches
    // would merely be serialised — and the test would agree for the wrong reason.
    let mock = publisher(&["published"], Duration::from_millis(300)).await;
    let clock = TestClock::new();
    let source = source_for(&mock, LONG_TTL, &clock);

    let kids: Vec<String> = (0..8).map(|i| format!("concurrent-{i}")).collect();
    let found = futures::future::join_all(kids.iter().map(|kid| source.key(kid))).await;

    assert!(
        found.iter().all(|key| key.as_ref().unwrap().is_none()),
        "none of them is published"
    );
    assert_eq!(
        fetches(&mock).await,
        1,
        "eight concurrent misses must single-flight onto one request. The cooldown cannot give \
         this: it is armed when a fetch starts, and these all check before that"
    );
}

// ============================================================================
// Expiry, and what the cooldown must not extend
// ============================================================================

#[tokio::test]
async fn an_expired_set_is_not_served_even_while_the_cooldown_holds() {
    let ttl = Duration::from_secs(60);
    let mock = publisher(&["rotating"], Duration::ZERO).await;
    let clock = TestClock::new();
    let source = source_for(&mock, ttl, &clock);

    assert!(source.key("rotating").await.unwrap().is_some(), "freshly fetched");

    // The publisher stops serving the key AND becomes unreachable, so a refetch
    // cannot replace the held set. The key must stop validating at the TTL all
    // the same (#361): the stolen-key window is the TTL, not the TTL plus a
    // cooldown.
    mock.reset().await;
    Mock::given(method("GET"))
        .and(path(JWKS_PATH))
        .respond_with(ResponseTemplate::new(503))
        .mount(&mock)
        .await;

    clock.advance(ttl + Duration::from_secs(1));

    let refused = source.key("rotating").await;
    assert!(
        matches!(refused, Err(JwksError::Unreachable { .. })) || refused.unwrap().is_none(),
        "past the TTL the held key is not served, whatever the publisher answers"
    );
    assert!(
        source.fresh_set().is_none(),
        "an expired set is not `fresh_set`, so nothing can read a rotated-out key off it"
    );
}

#[tokio::test]
async fn the_cooldown_never_exceeds_the_ttl() {
    let mock = publisher(&["k"], Duration::ZERO).await;
    let short = Duration::from_secs(5);
    let source = JwksSource::new(&format!("{}{JWKS_PATH}", mock.uri()), short).unwrap();
    assert_eq!(
        source.cooldown(),
        short,
        "an operator who shortens the TTL below the cooldown must not get a cache that expires \
         and then may not be refilled — every token refused for the difference, on a loop"
    );

    let source = JwksSource::new(&format!("{}{JWKS_PATH}", mock.uri()), LONG_TTL).unwrap();
    assert_eq!(source.cooldown(), REFETCH_COOLDOWN, "otherwise the constant is the bound");
}

// ============================================================================
// The operator's controls, which the cooldown does not gate
// ============================================================================

#[tokio::test]
async fn a_forced_refresh_is_not_subject_to_the_cooldown() {
    let mock = publisher(&["old"], Duration::ZERO).await;
    let clock = TestClock::new();
    let source = source_for(&mock, LONG_TTL, &clock);

    assert!(source.key("rotated-in").await.unwrap().is_none());
    assert_eq!(fetches(&mock).await, 1);

    mock.reset().await;
    mount(&mock, &["rotated-in"], Duration::ZERO).await;

    // No clock advance: the cooldown is fully in force.
    let count = source.refresh().await.expect("a forced refresh reaches the publisher");
    assert_eq!(count, 1, "refresh reports how many keys the publisher serves");
    assert_eq!(
        fetches(&mock).await,
        1,
        "the cooldown bounds refetches an unauthenticated sender can trigger. An operator \
         responding to a key compromise is neither, and must not be told to try again later"
    );
    assert!(source.key("rotated-in").await.unwrap().is_some(), "and the new key is now held");
}

#[tokio::test]
async fn invalidating_clears_the_cooldown_with_the_keys() {
    let mock = publisher(&["old"], Duration::ZERO).await;
    let clock = TestClock::new();
    let source = source_for(&mock, LONG_TTL, &clock);

    assert!(source.key("old").await.unwrap().is_some());
    assert_eq!(fetches(&mock).await, 1);

    mock.reset().await;
    mount(&mock, &["rotated-in"], Duration::ZERO).await;
    source.invalidate();

    assert!(
        source.key("rotated-in").await.unwrap().is_some(),
        "a flush means `do not serve these keys again`. Leaving the cooldown armed would answer \
         that by refusing every token until it lapsed, rather than by fetching"
    );
    assert!(source.key("old").await.unwrap().is_none(), "and the flushed key is gone");
}

// ============================================================================
// What may be fetched, and from where
// ============================================================================

#[test]
fn a_non_https_jwks_uri_is_refused_unless_it_is_loopback() {
    for uri in [
        "http://idp.example.com/.well-known/jwks.json",
        "ftp://idp/jwks",
    ] {
        let error = JwksSource::new(uri, LONG_TTL).expect_err("refused");
        assert!(
            matches!(error, JwksError::InvalidScheme { .. }),
            "{uri} must be refused by scheme, not accepted: a key set fetched in the clear can \
             be replaced in flight, which replaces who may sign a token"
        );
    }
    for uri in [
        "http://localhost:9999/.well-known/jwks.json",
        "http://127.0.0.1:9999/jwks",
        "https://idp.example.com/.well-known/jwks.json",
    ] {
        JwksSource::new(uri, LONG_TTL).unwrap_or_else(|error| panic!("{uri} accepted: {error}"));
    }
}

#[test]
fn a_jwks_uri_that_is_not_a_url_is_refused() {
    let error = JwksSource::new("not a url", LONG_TTL).expect_err("refused");
    assert!(matches!(error, JwksError::InvalidUrl { .. }));
}

#[tokio::test]
async fn an_address_the_shared_guard_blocks_is_refused_at_fetch() {
    // The loopback exemption is by *literal spelling*, and everything else goes
    // through `fraiseql_guard::net::is_blocked_ip`. These hosts resolve without
    // DNS (they are already addresses) and are refused on what they resolve to —
    // which is the rebinding shape in miniature: the check is on the address, not
    // on the name.
    //
    // `169.254.169.254` is the instance-metadata service. Five of the eight
    // hand-rolled guards this workspace used to carry accepted its IPv4-mapped
    // spelling (#776/#802); one guard is the reason this one cannot.
    for host in ["169.254.169.254", "127.0.0.2", "10.0.0.1"] {
        let source = JwksSource::new(&format!("https://{host}/jwks"), LONG_TTL)
            .expect("https is accepted at parse; the address is judged at fetch");
        let error = source.key("any").await.expect_err("the address guard refuses it");
        assert!(
            matches!(error, JwksError::Client { .. }),
            "an operator-supplied jwks_uri pointing at {host} must be refused when it is \
             resolved, got {error}"
        );
        assert!(
            error.to_string().contains(host),
            "and the refusal names the address, so the operator can see what it resolved to: \
             {error}"
        );
    }
}

#[tokio::test]
async fn an_oversized_key_set_is_refused_before_it_is_parsed() {
    let mock = MockServer::start().await;
    let filler = "A".repeat(crate::MAX_RESPONSE_BYTES + 1);
    Mock::given(method("GET"))
        .and(path(JWKS_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_string(filler))
        .mount(&mock)
        .await;
    let source = JwksSource::new(&format!("{}{JWKS_PATH}", mock.uri()), LONG_TTL).unwrap();

    let error = source.key("any").await.expect_err("refused");
    assert!(
        matches!(error, JwksError::TooLarge { .. }),
        "a compromised publisher must not be able to make this server allocate its response, \
         got {error}"
    );
}

#[tokio::test]
async fn a_response_that_is_not_a_key_set_is_refused_as_malformed() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(JWKS_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html>sign in</html>"))
        .mount(&mock)
        .await;
    let source = JwksSource::new(&format!("{}{JWKS_PATH}", mock.uri()), LONG_TTL).unwrap();

    let error = source.key("any").await.expect_err("refused");
    assert!(matches!(error, JwksError::Malformed { .. }), "got {error}");
}

// ============================================================================
// Which key types are usable, as ONE list
// ============================================================================

fn jwk(kty: &str, fields: serde_json::Value) -> Jwk {
    let mut object = json!({ "kty": kty, "kid": "k" });
    let (serde_json::Value::Object(base), serde_json::Value::Object(extra)) = (&mut object, fields)
    else {
        panic!("both are objects")
    };
    base.extend(extra);
    serde_json::from_value(object).expect("a JWK deserialises")
}

#[test]
fn rsa_and_ec_keys_are_both_usable() {
    jwk("RSA", json!({ "n": RSA_N, "e": "AQAB" }))
        .decoding_key()
        .expect("RSA is usable — the `[auth]` path accepted only this before #1335");
    jwk(
        "EC",
        json!({
            "crv": "P-256",
            "x":   "f83OJ3D2xF1Bg8vub9tLe1gHMzV76e8Tus9uPHvRVEU",
            "y":   "x_FEzRu9m36HLN_tue659LNpXW6pCyStikYjKIWI5a0",
        }),
    )
    .decoding_key()
    .expect(
        "EC is usable — the OAuth client accepted it, so an operator whose `IdP` rotated onto an \
         EC key saw one path work and the other refuse everything",
    );
}

#[test]
fn an_hmac_or_unknown_key_type_is_refused_by_name() {
    let error = jwk("oct", json!({ "k": "c2VjcmV0" })).decoding_key().expect_err("refused");
    let reason = error.to_string();
    assert!(
        reason.contains("oct") && reason.contains("shared secret"),
        "`oct` must be refused for its OWN reason — a secret served in a public key set cannot \
         attribute a signature to its publisher, which is the algorithm-confusion vector — and \
         not merely swept up by the unknown-type arm, which names the type too and so cannot \
         tell the two apart: {error}"
    );

    let error = jwk("OKP", json!({ "crv": "Ed25519", "x": "abc" }))
        .decoding_key()
        .expect_err("refused");
    assert!(
        error.to_string().contains("OKP"),
        "`EdDSA` is refused by name until something in this workspace verifies it: {error}"
    );
}

#[test]
fn an_rsa_key_missing_a_component_is_refused_rather_than_guessed() {
    let error = jwk("RSA", json!({ "n": RSA_N })).decoding_key().expect_err("refused");
    assert!(matches!(error, JwksError::UnusableKey { .. }), "got {error}");
}

#[test]
fn an_unfamiliar_key_alongside_a_usable_one_does_not_spoil_the_set() {
    // A publisher serves every key it has. Refusing the whole document because
    // one entry is of an unfamiliar kind would make an unrelated rotation break
    // authentication, so the set parses and the entry is only refused if a token
    // names it.
    let set: JwkSet = serde_json::from_value(json!({
        "keys": [
            { "kty": "OKP", "kid": "ed", "crv": "Ed25519", "x": "abc" },
            { "kty": "RSA", "kid": "rsa", "n": RSA_N, "e": "AQAB" },
        ]
    }))
    .expect("a set containing an unfamiliar key still parses");
    assert_eq!(set.keys.len(), 2);
    set.find("rsa").expect("still selectable").decoding_key().expect("still usable");
    assert!(set.find("ed").expect("kept").decoding_key().is_err(), "refused only when named");
}

#[test]
fn a_key_without_a_kid_is_kept_and_never_selected() {
    let set: JwkSet = serde_json::from_value(json!({
        "keys": [{ "kty": "RSA", "n": RSA_N, "e": "AQAB" }]
    }))
    .expect("kid is optional in the JWK spec");
    assert_eq!(set.keys.len(), 1, "kept");
    assert_eq!(set.kids().count(), 0, "and invisible to selection by kid");
}

// ============================================================================
// Selection, rotation, and the exact edge of the size cap
// ============================================================================

#[test]
fn a_key_is_selected_by_kid_and_only_by_kid() {
    let set: JwkSet = serde_json::from_value(json!({
        "keys": [
            { "kty": "RSA", "kid": "key1", "n": RSA_N, "e": "AQAB" },
            { "kty": "RSA", "kid": "key2", "n": RSA_N, "e": "AQAB" },
        ]
    }))
    .unwrap();
    assert!(set.find("key1").is_some());
    assert!(set.find("key2").is_some());
    assert!(set.find("key3").is_none(), "a kid the set does not publish selects nothing");
    assert!(set.find("").is_none(), "and neither does an empty one");
}

#[test]
fn a_withdrawn_key_is_reported_as_a_rotation_and_an_added_one_is_not() {
    let held: JwkSet = serde_json::from_value(json!({
        "keys": [
            { "kty": "RSA", "kid": "old-1", "n": RSA_N, "e": "AQAB" },
            { "kty": "RSA", "kid": "old-2", "n": RSA_N, "e": "AQAB" },
        ]
    }))
    .unwrap();

    let one_withdrawn: JwkSet = serde_json::from_value(json!({
        "keys": [
            { "kty": "RSA", "kid": "old-1", "n": RSA_N, "e": "AQAB" },
            { "kty": "RSA", "kid": "new-1", "n": RSA_N, "e": "AQAB" },
        ]
    }))
    .unwrap();
    assert_eq!(
        held.kids_missing_from(&one_withdrawn),
        vec!["old-2"],
        "a key the publisher has withdrawn is the rotation, and it is named"
    );

    let only_added: JwkSet = serde_json::from_value(json!({
        "keys": [
            { "kty": "RSA", "kid": "old-1", "n": RSA_N, "e": "AQAB" },
            { "kty": "RSA", "kid": "old-2", "n": RSA_N, "e": "AQAB" },
            { "kty": "RSA", "kid": "new-1", "n": RSA_N, "e": "AQAB" },
        ]
    }))
    .unwrap();
    assert!(
        held.kids_missing_from(&only_added).is_empty(),
        "publishing a key alongside the existing ones is the normal shape of a rotation \
         *starting* and is not itself a withdrawal — reporting it would train an operator to \
         ignore the warning"
    );

    assert!(
        JwkSet::default().kids_missing_from(&held).is_empty(),
        "and holding nothing cannot be a withdrawal"
    );
}

#[tokio::test]
async fn a_refetch_that_drops_a_key_stops_that_key_verifying() {
    // #361 at its source. The held set is replaced wholesale rather than merged,
    // so a key the publisher withdrew cannot keep validating off what is held —
    // which it would if a refetch only added what it found.
    let mock = publisher(&["rotated-out", "kept"], Duration::ZERO).await;
    let clock = TestClock::new();
    let source = source_for(&mock, LONG_TTL, &clock);

    assert!(source.key("rotated-out").await.unwrap().is_some(), "published at first");

    mock.reset().await;
    mount(&mock, &["kept"], Duration::ZERO).await;
    source.refresh().await.expect("the publisher is reachable");

    assert!(
        source.key("rotated-out").await.unwrap().is_none(),
        "a withdrawn key must stop verifying tokens; a refetch that merged would leave it \
         usable for as long as the process lived"
    );
    assert!(
        source.key("kept").await.unwrap().is_some(),
        "and the remaining key is unaffected"
    );
}

#[tokio::test]
async fn a_key_set_exactly_at_the_size_cap_is_accepted() {
    // The other side of `an_oversized_key_set_is_refused_before_it_is_parsed`.
    // Without it, `>` and `>=` are indistinguishable — and a cap that refuses the
    // document at exactly the limit is a cap nobody can sit under.
    let mock = MockServer::start().await;
    let head = format!(r#"{{"keys":[{{"kty":"RSA","kid":"pad","n":"{RSA_N}","e":"AQAB","x5c":[""#);
    let tail = r#""]}]}"#;
    let padding = crate::MAX_RESPONSE_BYTES - head.len() - tail.len();
    let body = format!("{head}{}{tail}", "A".repeat(padding));
    assert_eq!(body.len(), crate::MAX_RESPONSE_BYTES, "the fixture sits exactly on the cap");

    Mock::given(method("GET"))
        .and(path(JWKS_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .mount(&mock)
        .await;
    let source = JwksSource::new(&format!("{}{JWKS_PATH}", mock.uri()), LONG_TTL).unwrap();

    assert!(
        source.key("pad").await.unwrap().is_some(),
        "a document of exactly MAX_RESPONSE_BYTES is within the cap, not over it"
    );
}

// ============================================================================
// Resolve-and-pin: the two halves that close the DNS-rebinding window
// ============================================================================

#[tokio::test]
async fn a_public_address_is_returned_for_pinning() {
    // The counterweight to `an_address_the_shared_guard_blocks_is_refused_at_fetch`.
    // A guard that refused everything would pass that test and serve nobody. An IP
    // literal resolves to itself, so this needs no DNS.
    let addrs = crate::fetch::resolve_and_guard("8.8.8.8", 443)
        .await
        .expect("a globally-routable address is not blocked");
    assert!(
        addrs
            .iter()
            .any(|addr| addr.ip().to_string() == "8.8.8.8" && addr.port() == 443),
        "and it is handed back so the connection can be pinned to it: {addrs:?}"
    );
}

#[tokio::test]
async fn a_private_or_loopback_address_is_refused_before_any_connection() {
    for host in ["10.0.0.1", "127.0.0.1", "169.254.169.254"] {
        let refused = crate::fetch::resolve_and_guard(host, 443).await;
        let error = refused.expect_err("must be refused");
        assert!(error.contains(host), "the refusal names the host: {error}");
    }
}

#[tokio::test]
async fn the_pinned_client_connects_to_the_validated_address_and_not_through_dns() {
    // The mechanism, not the intent: the client is pinned to a synthetic host that
    // has no DNS record at all, so the request can only arrive if reqwest used the
    // pin instead of its own resolver. That is what makes the check-then-connect
    // window unexploitable — validating and re-resolving would let the name answer
    // public for the check and private for the connect.
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/pinned"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&mock)
        .await;

    let addr = *mock.address();
    let client =
        crate::fetch::pinned_client("jwks.pinned.invalid", &[addr], Duration::from_secs(5))
            .expect("the pinned client builds");
    let response = client
        .get(format!("http://jwks.pinned.invalid:{}/pinned", addr.port()))
        .send()
        .await
        .expect("the pin, not DNS, is what resolved this");
    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "ok");
}

// ============================================================================
// What a source is before it has fetched anything, and what it says about itself
// ============================================================================

#[test]
fn a_source_holds_nothing_before_its_first_lookup() {
    // Construction does no I/O: a server whose `IdP` is briefly unreachable still
    // boots, and the first token pays for the fetch.
    let source = JwksSource::new("https://idp.example.com/.well-known/jwks.json", LONG_TTL)
        .expect("accepted");
    assert!(source.fresh_set().is_none());
}

#[test]
fn the_debug_rendering_names_the_endpoint_and_never_a_key() {
    let source =
        JwksSource::new("https://idp.example.com/.well-known/jwks.json", LONG_TTL).unwrap();
    let rendered = format!("{source:?}");
    assert!(rendered.contains("JwksSource"), "{rendered}");
    assert!(
        rendered.contains("idp.example.com"),
        "the endpoint is the useful part: {rendered}"
    );
    assert!(rendered.contains("held_keys"), "as is how many keys are held: {rendered}");
}

#[tokio::test]
async fn a_ttl_of_zero_means_never_cache_and_not_never_verify() {
    // An operator may legitimately choose to hold nothing between requests —
    // expensive, and the shortest possible stolen-key window. What that must NOT
    // mean is that every token is refused because the set fetched a moment ago
    // reads as already expired. The call that fetches is answered by what it
    // fetched.
    let mock = publisher(&["always-refetched"], Duration::ZERO).await;
    let source = JwksSource::new(&format!("{}{JWKS_PATH}", mock.uri()), Duration::ZERO)
        .expect("accepted");

    for attempt in 0..3 {
        assert!(
            source.key("always-refetched").await.unwrap().is_some(),
            "attempt {attempt} must verify: a zero TTL is `do not keep the keys`, not `do not \
             use them`"
        );
    }
    assert_eq!(
        fetches(&mock).await,
        3,
        "and it really does refetch each time — the cooldown is clamped to the TTL, so a zero \
         TTL cannot leave a request unable to look"
    );
    assert!(source.fresh_set().is_none(), "nothing is held between requests");
}
