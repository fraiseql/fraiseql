#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::time::Duration;

use uuid::Uuid;

use super::*;
use crate::event::{ChangeEvent, ChangeOp};

#[test]
fn sink_kind_serde_is_kebab() {
    assert_eq!(serde_json::to_string(&SinkKind::NatsJetStream).unwrap(), "\"nats-jetstream\"");
    assert_eq!(
        serde_json::from_str::<SinkKind>("\"nats-jetstream\"").unwrap(),
        SinkKind::NatsJetStream
    );
}

#[test]
fn render_subject_happy_path() {
    let tenant = Uuid::from_u128(0xab);
    let ev = ChangeEvent::new(1, "tb_post", ChangeOp::Insert).with_tenant(tenant);
    assert_eq!(
        render_subject("fraiseql.{tenant_id}.{table}", &ev).unwrap(),
        format!("fraiseql.{tenant}.tb_post")
    );
}

#[test]
fn render_subject_none_tenant_renders_placeholder() {
    let ev = ChangeEvent::new(1, "tb_post", ChangeOp::Insert);
    assert_eq!(
        render_subject("fraiseql.{tenant_id}.{table}", &ev).unwrap(),
        "fraiseql._none_.tb_post"
    );
}

#[test]
fn render_subject_op_placeholder() {
    let ev = ChangeEvent::new(1, "tb_post", ChangeOp::Update);
    assert_eq!(render_subject("c.{table}.{op}", &ev).unwrap(), "c.tb_post.update");
}

#[test]
fn render_subject_rejects_nats_illegal_table_chars() {
    for bad in ["tb.post", "tb*post", "tb>post", "tb post", "tb\tpost"] {
        let ev = ChangeEvent::new(1, bad, ChangeOp::Insert);
        assert!(
            render_subject("fraiseql.{tenant_id}.{table}", &ev).is_err(),
            "should reject illegal table segment {bad:?}"
        );
    }
}

#[test]
fn matches_allows_everything_with_no_filter() {
    let cfg = CdcSinkConfig::new("s", "fraiseql.{table}");
    assert!(cfg.matches(&ChangeEvent::new(1, "tb_post", ChangeOp::Insert)));
}

#[test]
fn matches_table_allowlist() {
    let cfg = CdcSinkConfig::new("s", "t").with_tables(vec!["tb_post".to_owned()]);
    assert!(cfg.matches(&ChangeEvent::new(1, "tb_post", ChangeOp::Insert)));
    assert!(!cfg.matches(&ChangeEvent::new(1, "tb_user", ChangeOp::Insert)));
}

#[test]
fn matches_tenant_allowlist_rejects_other_and_unstamped() {
    let allowed = Uuid::from_u128(1);
    let cfg = CdcSinkConfig::new("s", "t").with_tenants(vec![allowed]);
    assert!(cfg.matches(&ChangeEvent::new(1, "tb_post", ChangeOp::Insert).with_tenant(allowed)));
    assert!(!cfg.matches(
        &ChangeEvent::new(1, "tb_post", ChangeOp::Insert).with_tenant(Uuid::from_u128(2))
    ));
    assert!(!cfg.matches(&ChangeEvent::new(1, "tb_post", ChangeOp::Insert)));
}

#[test]
fn publish_outcome_distinguishes_transient_and_permanent() {
    assert_ne!(
        PublishOutcome::Transient("x".to_owned()),
        PublishOutcome::Permanent("x".to_owned())
    );
    assert_ne!(PublishOutcome::Published, PublishOutcome::Transient("x".to_owned()));
}

#[test]
fn backoff_is_monotonic_and_capped() {
    assert_eq!(next_attempt_delay(0), Duration::from_secs(1));
    assert_eq!(next_attempt_delay(1), Duration::from_secs(1));
    assert_eq!(next_attempt_delay(2), Duration::from_secs(2));
    assert_eq!(next_attempt_delay(3), Duration::from_secs(4));
    assert_eq!(next_attempt_delay(9), Duration::from_secs(256));
    assert_eq!(next_attempt_delay(20), Duration::from_mins(5));
    assert_eq!(next_attempt_delay(u32::MAX), Duration::from_mins(5));
}

// ── Kafka topic charset ───────────────────────────────────────────────────────

#[test]
fn kafka_topic_accepts_the_legal_charset() {
    let at_cap = "x".repeat(249);
    for topic in ["fraiseql.tb_post", "a", "A-Z.0-9_x", at_cap.as_str()] {
        assert!(validate_kafka_topic(topic).is_ok(), "should accept {topic:?}");
    }
}

#[test]
fn kafka_topic_rejects_chars_nats_would_accept() {
    // NATS subjects allow far more than `[a-zA-Z0-9._-]`, so a template that
    // renders cleanly for NATS can be illegal in Kafka.
    for topic in [
        "fraiseql/tb_post",
        "fraiseql:tb_post",
        "fraiseql+tb_post",
        "tb post",
        "tb\u{e9}",
    ] {
        assert!(validate_kafka_topic(topic).is_err(), "should reject {topic:?}");
    }
}

#[test]
fn kafka_topic_rejects_empty_dot_dotdot_and_overlong() {
    for topic in ["", ".", ".."] {
        assert!(validate_kafka_topic(topic).is_err(), "should reject {topic:?}");
    }
    assert!(validate_kafka_topic(&"x".repeat(250)).is_err(), "249 is the Kafka cap");
}

#[test]
fn render_kafka_topic_rejects_a_nats_legal_but_kafka_illegal_render() {
    let ev = ChangeEvent::new(1, "tb_post", ChangeOp::Insert);
    // `/` passes the NATS sanitiser and is illegal in a Kafka topic.
    assert!(render_kafka_topic("fraiseql/{table}", &ev).is_err());
    assert_eq!(render_kafka_topic("fraiseql.{table}", &ev).unwrap(), "fraiseql.tb_post");
}

#[test]
fn render_kafka_topic_rejects_an_injected_separator_before_kafka_sees_it() {
    // The NATS sanitiser already refuses a `.` inside an interpolated segment;
    // assert it still holds on the Kafka path, where `.` is a *legal* topic
    // character and so would otherwise pass the charset check unnoticed.
    let ev = ChangeEvent::new(1, "tb_post.evil", ChangeOp::Insert);
    assert!(render_kafka_topic("fraiseql.{table}", &ev).is_err());
}

// ── Shared partition key ──────────────────────────────────────────────────────

#[test]
fn entity_partition_key_pins_one_entity_to_one_partition() {
    // Both Kafka and Kinesis hash this key to choose a partition/shard, and both
    // order only *within* one. Keying by anything unique per message (`seq`, say)
    // would scatter an entity's changes across every partition and destroy the
    // ordering the sinks exist to preserve — so the key is the entity's identity.
    let id = Uuid::from_u128(0xfeed);
    let insert = ChangeEvent::new(1, "tb_post", ChangeOp::Insert).with_object_id(id);
    let update = ChangeEvent::new(9_999, "tb_post", ChangeOp::Update).with_object_id(id);
    assert_eq!(
        entity_partition_key(&insert),
        entity_partition_key(&update),
        "two changes to one entity must land on one partition"
    );
    assert_eq!(entity_partition_key(&insert), format!("tb_post:{id}"));
    assert!(
        !entity_partition_key(&update).contains("9999"),
        "the seq must not appear in the key — it would scatter an entity across partitions"
    );

    let other = ChangeEvent::new(2, "tb_post", ChangeOp::Insert).with_object_id(Uuid::from_u128(2));
    assert_ne!(
        entity_partition_key(&insert),
        entity_partition_key(&other),
        "distinct entities must be free to spread across partitions"
    );
}

#[test]
fn entity_partition_key_degrades_to_the_table_without_an_object_id() {
    // Deterministic — never null, never random — so a row with no `object_id`
    // still orders consistently, just per-table rather than per-entity.
    let ev = ChangeEvent::new(1, "tb_post", ChangeOp::Insert);
    assert_eq!(entity_partition_key(&ev), "tb_post");
    assert_eq!(entity_partition_key(&ev), entity_partition_key(&ev));
}

// ── Kinesis endpoint + region guard ───────────────────────────────────────────
//
// Pure, and here for the same reason the Kafka guard is: no aws-sdk type appears
// in these signatures, so the refusing half runs in the always-compiled leg.

/// Run `f` with the Kinesis plaintext opt-in and endpoint override both absent.
fn without_kinesis_optin<T>(f: impl FnOnce() -> T) -> T {
    temp_env::with_vars(
        [
            ("FRAISEQL_KINESIS_ALLOW_PLAINTEXT", None::<&str>),
            ("FRAISEQL_KINESIS_ENDPOINT_URL", None),
        ],
        f,
    )
}

/// Run `f` with an endpoint-URL override set and the plaintext opt-in absent.
fn with_kinesis_url<T>(url: &str, f: impl FnOnce() -> T) -> T {
    temp_env::with_vars(
        [
            ("FRAISEQL_KINESIS_ENDPOINT_URL", Some(url)),
            ("FRAISEQL_KINESIS_ALLOW_PLAINTEXT", None),
        ],
        f,
    )
}

/// Run `f` with an override set, opted in to plaintext, in a declared dev env.
fn with_kinesis_url_optin<T>(url: &str, f: impl FnOnce() -> T) -> T {
    temp_env::with_vars(
        [
            ("FRAISEQL_KINESIS_ENDPOINT_URL", Some(url)),
            ("FRAISEQL_KINESIS_ALLOW_PLAINTEXT", Some("true")),
            ("FRAISEQL_ENV", Some("development")),
            ("FRAISEQL_PROFILE", None),
            ("KUBERNETES_SERVICE_HOST", None),
        ],
        f,
    )
}

#[test]
fn kinesis_endpoint_refuses_scheme_less_input() {
    // A bare region is what an operator reaches for first. Requiring the scheme
    // keeps the sink kind unambiguous and matches the Kafka endpoint's contract.
    // Each refusal is paired with the same value *plus* a scheme, so the failure
    // is attributable to the missing scheme and to nothing else.
    for region in ["us-east-1", "eu-west-3", "ap-southeast-2"] {
        assert!(
            guard_kinesis_endpoint(region).is_err(),
            "a scheme-less endpoint must be refused, not defaulted: {region}"
        );
        assert!(
            guard_kinesis_endpoint(&format!("kinesis://{region}")).is_ok(),
            "control: the same region must be accepted once a scheme is given"
        );
    }
}

#[test]
fn kinesis_endpoint_refuses_unknown_schemes() {
    // Every value here carries a legal region, so an unknown scheme that fell
    // through to "accept" would pass — the refusal is attributable to the scheme.
    for endpoint in [
        "https://us-east-1",
        "kafka://us-east-1",
        "nats://us-east-1",
        "kinesis+ssl://us-east-1",
        "aws-kinesis://us-east-1",
        "://us-east-1",
    ] {
        assert!(
            guard_kinesis_endpoint(endpoint).is_err(),
            "only kinesis:// is supported: {endpoint}"
        );
    }
    assert!(guard_kinesis_endpoint("kinesis://us-east-1").is_ok(), "control");
}

#[test]
fn kinesis_endpoint_extracts_and_normalises_the_region() {
    // AWS region identifiers are canonically lowercase; normalising here beats a
    // confusing SignatureDoesNotMatch from the far end.
    assert_eq!(guard_kinesis_endpoint("kinesis://us-east-1").unwrap().region, "us-east-1");
    assert_eq!(guard_kinesis_endpoint("KINESIS://US-East-1").unwrap().region, "us-east-1");
    assert_eq!(
        guard_kinesis_endpoint("kinesis://us-gov-west-1").unwrap().region,
        "us-gov-west-1"
    );
}

#[test]
fn kinesis_endpoint_refuses_an_empty_or_illegal_region() {
    // The region is interpolated into the endpoint the SDK resolves, so an
    // unconstrained value is an injection seam rather than merely a typo.
    for endpoint in [
        "kinesis://",
        "kinesis://us east 1",
        "kinesis://us_east_1",
        "kinesis://us-east-1.evil.example.com",
        "kinesis://-us-east-1",
        "kinesis://us-east-1:443",
    ] {
        assert!(
            guard_kinesis_endpoint(endpoint).is_err(),
            "an AWS region is [a-z0-9-] starting with a letter: {endpoint}"
        );
    }
}

#[test]
fn kinesis_endpoint_refuses_userinfo_path_and_query_components() {
    // None is legal in a region, and each can mask the value actually used.
    for endpoint in [
        "kinesis://user@us-east-1",
        "kinesis://us-east-1/stream",
        "kinesis://us-east-1?region=eu-west-1",
        "kinesis://us-east-1#eu-west-1",
    ] {
        assert!(guard_kinesis_endpoint(endpoint).is_err(), "must refuse {endpoint}");
    }
}

// ── Kinesis endpoint-URL override ─────────────────────────────────────────────

#[test]
fn kinesis_endpoint_url_absent_means_the_aws_resolver() {
    // No override is the production shape: the SDK resolves the real regional
    // endpoint, which is HTTPS. `None` here must not be an error.
    without_kinesis_optin(|| {
        assert_eq!(resolve_kinesis_endpoint_url().unwrap(), None);
    });
    // An override set to whitespace is treated as absent, not as a malformed URL.
    with_kinesis_url("   ", || {
        assert_eq!(resolve_kinesis_endpoint_url().unwrap(), None);
    });
}

#[test]
fn kinesis_endpoint_url_refuses_plaintext_without_the_optin() {
    // Change events carry the full row after-image; http:// puts them on the wire
    // in the clear. Loopback hosts are used deliberately — the refusal must come
    // from the missing opt-in, not from host screening.
    for url in ["http://localhost:4566", "http://127.0.0.1:4566"] {
        with_kinesis_url(url, || {
            assert!(resolve_kinesis_endpoint_url().is_err(), "plaintext needs the opt-in: {url}");
        });
        with_kinesis_url_optin(url, || {
            assert!(
                resolve_kinesis_endpoint_url().is_ok(),
                "control: the same URL is accepted once opted in: {url}"
            );
        });
    }
}

#[test]
fn kinesis_plaintext_optin_is_inert_in_production() {
    temp_env::with_vars(
        [
            ("FRAISEQL_KINESIS_ENDPOINT_URL", Some("http://localhost:4566")),
            ("FRAISEQL_KINESIS_ALLOW_PLAINTEXT", Some("true")),
            ("FRAISEQL_ENV", Some("production")),
        ],
        || {
            assert!(resolve_kinesis_endpoint_url().is_err());
        },
    );
}

#[test]
fn kinesis_plaintext_optin_is_screened_not_unrestricted() {
    // The opt-in exists to reach a dev LocalStack — on localhost, or on a CI bind
    // hostname when the suite runs in a container network. It must not double as a
    // licence to reach the instance-metadata service or an internal network.
    //
    // These are exactly the Kafka guard's plaintext-path semantics, deliberately:
    // the two flags must not disagree about what the operator asked for.
    for url in [
        "http://localhost:4566",
        "http://127.0.0.1:4566",
        "http://[::1]:4566",
        // The Dagger shape: a service bound as `localstack` in the container
        // network. Not loopback, and blocked by none of the screened classes.
        "http://localstack:4566",
    ] {
        with_kinesis_url_optin(url, || {
            assert!(resolve_kinesis_endpoint_url().is_ok(), "dev LocalStack must work: {url}");
        });
    }
    for url in [
        "http://169.254.169.254",
        "http://metadata.google.internal",
        "http://10.0.0.5:4566",
        "http://192.168.1.10:4566",
    ] {
        with_kinesis_url_optin(url, || {
            assert!(
                resolve_kinesis_endpoint_url().is_err(),
                "the plaintext opt-in must not reach {url}"
            );
        });
    }
}

#[test]
fn kinesis_refuses_every_blocked_corpus_entry_even_when_opted_in() {
    // The corpus test that would have caught #816, on this transport.
    use fraiseql_guard::net::vectors::{MUST_BLOCK, url_host};
    for (addr, why) in MUST_BLOCK {
        if fraiseql_guard::net::is_loopback_host(addr) {
            continue;
        }
        let url = format!("http://{}:4566", url_host(addr));
        with_kinesis_url_optin(&url, || {
            assert!(resolve_kinesis_endpoint_url().is_err(), "must refuse {addr} ({why})");
        });
    }
}

#[test]
fn https_endpoint_url_permits_private_range_by_design() {
    // Pinned so it is not "fixed" into a regression, exactly as for kafka+ssl://:
    // a VPC interface endpoint for Kinesis resolves into RFC 1918 space, which
    // `blocked_host_reason` refuses. Screening belongs on the plaintext escape
    // hatch, not on an encrypted endpoint the operator chose.
    for url in [
        "https://vpce-0abc-kinesis.us-east-1.vpce.amazonaws.com",
        "https://10.0.1.5:443",
        "https://kinesis.eu-west-1.amazonaws.com",
    ] {
        with_kinesis_url(url, || {
            assert!(
                resolve_kinesis_endpoint_url().is_ok(),
                "an encrypted endpoint may point into private space: {url}"
            );
        });
    }
}

#[test]
fn kinesis_endpoint_url_refuses_scheme_less_and_unknown_schemes() {
    // Opted in throughout, and every host is loopback, so neither the plaintext
    // policy nor host screening can be what refuses these.
    for url in [
        "localhost:4566",
        "ftp://localhost:4566",
        "ws://localhost:4566",
        "://localhost",
    ] {
        with_kinesis_url_optin(url, || {
            assert!(resolve_kinesis_endpoint_url().is_err(), "must refuse {url}");
        });
    }
    with_kinesis_url_optin("http://localhost:4566", || {
        assert!(resolve_kinesis_endpoint_url().is_ok(), "control");
    });
}

#[test]
fn kinesis_endpoint_url_refuses_userinfo_and_credentials_in_the_url() {
    // `http://user:pw@host` is the shape that masked a host before #816.
    with_kinesis_url_optin("http://user:pw@localhost:4566", || {
        assert!(resolve_kinesis_endpoint_url().is_err());
    });
}

// ── Kinesis stream-name validation ────────────────────────────────────────────

#[test]
fn kinesis_stream_accepts_the_legal_charset() {
    for name in [
        "fraiseql",
        "fraiseql.tb_post",
        "fraiseql-changes_1",
        "a",
        &"x".repeat(128),
    ] {
        assert!(validate_kinesis_stream(name).is_ok(), "must accept {name}");
    }
}

#[test]
fn kinesis_stream_rejects_illegal_chars_and_overlong_names() {
    // Kinesis allows [a-zA-Z0-9_.-] only and caps names at 128 — a *narrower*
    // cap than Kafka's 249, so a template legal for the Kafka sink can be illegal
    // here. Hence a separate check rather than reuse of the Kafka validator.
    for name in [
        "",
        "fraiseql/post",
        "fraiseql post",
        "fraiseql:post",
        "fraiseql*",
        "frais€ql",
    ] {
        assert!(validate_kinesis_stream(name).is_err(), "must reject {name:?}");
    }
    assert!(
        validate_kinesis_stream(&"x".repeat(129)).is_err(),
        "128 is the Kinesis cap, and it is not Kafka's 249"
    );
    assert!(validate_kafka_topic(&"x".repeat(129)).is_ok(), "control: legal for Kafka");
}

#[test]
fn render_kinesis_stream_rejects_an_injected_separator_before_kinesis_sees_it() {
    // `.` is a legal Kinesis stream character, so a crafted tenant/table value
    // carrying one would sail through the charset check; the NATS sanitiser in
    // `render_subject` is what stops it escaping into another stream namespace.
    let ev = ChangeEvent::new(1, "tb_post.evil", ChangeOp::Insert);
    assert!(render_kinesis_stream("fraiseql.{table}", &ev).is_err());

    let ok = ChangeEvent::new(1, "tb_post", ChangeOp::Insert);
    assert_eq!(render_kinesis_stream("fraiseql.{table}", &ok).unwrap(), "fraiseql.tb_post");
}
