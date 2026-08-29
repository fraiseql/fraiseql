//! The Kafka endpoint guard's own tests, in both directions.
//!
//! Every refusal has an acceptance beside it and every acceptance a refusal, because a
//! guard tested one way is how #816 shipped inverted: the NATS transport refused what it
//! should have permitted and permitted what it should have refused, and a one-direction
//! suite was green throughout.
//!
//! The guard names no rdkafka type, so all of this runs in the cheap always-compiled
//! leg rather than only where a broker feature is enabled.

#![allow(clippy::unwrap_used, clippy::expect_used)] // Reason: test code.

use super::*;

/// Run `f` with the Kafka plaintext opt-in explicitly absent.
fn without_kafka_optin<T>(f: impl FnOnce() -> T) -> T {
    temp_env::with_var_unset("FRAISEQL_KAFKA_ALLOW_PLAINTEXT", f)
}

/// Run `f` opted in to plaintext and in a declared development environment.
fn with_kafka_optin<T>(f: impl FnOnce() -> T) -> T {
    temp_env::with_vars(
        [
            ("FRAISEQL_KAFKA_ALLOW_PLAINTEXT", Some("true")),
            ("FRAISEQL_ENV", Some("development")),
            ("FRAISEQL_PROFILE", None),
            ("KUBERNETES_SERVICE_HOST", None),
        ],
        f,
    )
}

#[test]
fn kafka_endpoint_refuses_scheme_less_input() {
    // `bootstrap.servers` has no scheme, so a bare list is what an operator will
    // reach for first. librdkafka would silently treat it as PLAINTEXT; refusing
    // beats defaulting (the #816 lesson, in the one shape Kafka can express it).
    //
    // Asserted *with* the plaintext opt-in on, and against loopback hosts, so the
    // only thing that can produce the refusal is the missing scheme. Run without
    // the opt-in this test passes even when the scheme check is deleted entirely —
    // the plaintext policy refuses it first, and the assertion proves nothing.
    with_kafka_optin(|| {
        for endpoint in [
            "localhost:9092",
            "127.0.0.1:9092,localhost:9093",
            "[::1]:9092",
        ] {
            assert!(
                guard_kafka_endpoint(endpoint).is_err(),
                "a scheme-less endpoint must be refused, not defaulted: {endpoint}"
            );
            // The same brokers *with* a scheme are accepted, so the refusal above
            // is attributable to the scheme and nothing else.
            assert!(
                guard_kafka_endpoint(&format!("kafka://{endpoint}")).is_ok(),
                "control: the same brokers must be accepted once a scheme is given"
            );
        }
    });
}

#[test]
fn kafka_endpoint_refuses_unknown_schemes() {
    // Opted in, loopback hosts: an unknown scheme that fell through to Plaintext
    // would be *accepted* here, so the refusal is attributable to the scheme match.
    // Without the opt-in this passes even with the match arm deleted.
    with_kafka_optin(|| {
        for endpoint in [
            "ssl://localhost:9092",
            "http://localhost:9092",
            "nats://localhost:9092",
            "kafka+sasl-plaintext://localhost:9092",
            "kafka+ssl+sasl://localhost:9092",
            "://localhost:9092",
        ] {
            assert!(
                guard_kafka_endpoint(endpoint).is_err(),
                "only kafka://, kafka+ssl:// and kafka+sasl-ssl:// are supported: {endpoint}"
            );
        }
    });
}

#[test]
fn kafka_endpoint_maps_scheme_to_an_explicit_security_protocol() {
    // librdkafka's own default is PLAINTEXT, so the protocol must be set from the
    // scheme rather than left unset.
    without_kafka_optin(|| {
        let ssl = guard_kafka_endpoint("kafka+ssl://b1:9092,b2:9092").unwrap();
        assert_eq!(ssl.security_protocol, KafkaSecurityProtocol::Ssl);
        assert_eq!(ssl.security_protocol.as_str(), "ssl");

        let sasl = guard_kafka_endpoint("kafka+sasl-ssl://b1:9092").unwrap();
        assert_eq!(sasl.security_protocol, KafkaSecurityProtocol::SaslSsl);
        assert_eq!(sasl.security_protocol.as_str(), "sasl_ssl");
    });

    with_kafka_optin(|| {
        let plain = guard_kafka_endpoint("kafka://localhost:9092").unwrap();
        assert_eq!(plain.security_protocol, KafkaSecurityProtocol::Plaintext);
        assert_eq!(plain.security_protocol.as_str(), "plaintext");
    });
}

#[test]
fn kafka_endpoint_strips_the_scheme_from_bootstrap_servers() {
    without_kafka_optin(|| {
        let ep =
            guard_kafka_endpoint("kafka+ssl://b1.example.com:9092, b2.example.com:9093").unwrap();
        assert_eq!(ep.bootstrap_servers, "b1.example.com:9092,b2.example.com:9093");
    });
}

#[test]
fn kafka_endpoint_is_case_insensitive_in_the_scheme() {
    without_kafka_optin(|| {
        let ep = guard_kafka_endpoint("KAFKA+SSL://B1.Example.COM:9092").unwrap();
        assert_eq!(ep.security_protocol, KafkaSecurityProtocol::Ssl);
    });
}

#[test]
fn kafka_endpoint_refuses_plaintext_without_the_optin() {
    without_kafka_optin(|| {
        for endpoint in ["kafka://broker.example.com:9092", "kafka://localhost:9092"] {
            assert!(
                guard_kafka_endpoint(endpoint).is_err(),
                "plaintext kafka:// carries the full row after-image in the clear: {endpoint}"
            );
        }
    });
}

#[test]
fn kafka_plaintext_optin_is_inert_in_production() {
    temp_env::with_vars(
        [
            ("FRAISEQL_KAFKA_ALLOW_PLAINTEXT", Some("true")),
            ("FRAISEQL_ENV", Some("production")),
        ],
        || {
            assert!(guard_kafka_endpoint("kafka://localhost:9092").is_err());
        },
    );
}

#[test]
fn kafka_plaintext_optin_permits_loopback_brokers() {
    with_kafka_optin(|| {
        for endpoint in [
            "kafka://localhost:9092",
            "kafka://127.0.0.1:9092",
            "kafka://[::1]:9092",
            "kafka://localhost:9092,127.0.0.1:9093",
        ] {
            assert!(
                guard_kafka_endpoint(endpoint).is_ok(),
                "the opt-in exists to reach a dev broker on localhost: {endpoint}"
            );
        }
    });
}

#[test]
fn kafka_plaintext_refuses_the_whole_list_when_any_broker_is_blocked() {
    // The property NATS has no equivalent for: it takes one URL, Kafka takes a
    // list. Guarding only the first entry would let a metadata address ride along
    // behind a loopback broker, and librdkafka contacts every bootstrap server.
    with_kafka_optin(|| {
        for endpoint in [
            "kafka://169.254.169.254:9092",
            "kafka://localhost:9092,169.254.169.254:9092",
            "kafka://169.254.169.254:9092,localhost:9092",
            "kafka://localhost:9092,localhost:9093,metadata.google.internal:9092,localhost:9094",
        ] {
            assert!(
                guard_kafka_endpoint(endpoint).is_err(),
                "one blocked broker must refuse the whole endpoint: {endpoint}"
            );
        }
    });
}

#[test]
fn kafka_endpoint_refuses_userinfo_and_path_components() {
    // Neither is legal in `bootstrap.servers`. Accepting them would let a host be
    // masked the way `nats://user:pw@host` masked one before #816.
    with_kafka_optin(|| {
        for endpoint in [
            "kafka://user:pw@169.254.169.254:9092",
            "kafka://localhost:9092/path",
            "kafka://localhost:9092?x=1",
            "kafka://localhost:9092#frag",
        ] {
            assert!(
                guard_kafka_endpoint(endpoint).is_err(),
                "userinfo/path components are not valid bootstrap servers: {endpoint}"
            );
        }
    });
}

#[test]
fn kafka_endpoint_refuses_empty_broker_entries() {
    without_kafka_optin(|| {
        for endpoint in [
            "kafka+ssl://",
            "kafka+ssl://b:9092,",
            "kafka+ssl://,b:9092",
            "kafka+ssl://b:9092,,c:9092",
        ] {
            assert!(
                guard_kafka_endpoint(endpoint).is_err(),
                "an empty broker entry must not be silently dropped: {endpoint}"
            );
        }
    });
}

#[test]
fn ssl_endpoints_permit_private_range_brokers_by_design() {
    // Deliberate, and pinned so it is not "fixed" into a regression: MSK and every
    // VPC-hosted cluster put brokers in RFC 1918 space, which `is_blocked_ip`
    // refuses. Host screening therefore applies on the *plaintext opt-in* path,
    // where it stops the escape hatch doubling as an SSRF licence — not to an
    // encrypted endpoint the operator explicitly pointed at their own network.
    without_kafka_optin(|| {
        for endpoint in [
            "kafka+ssl://10.0.1.5:9092,10.0.2.5:9092",
            "kafka+sasl-ssl://b-1.msk.eu-west-1.amazonaws.com:9096",
            "kafka+ssl://192.168.1.10:9092",
        ] {
            assert!(
                guard_kafka_endpoint(endpoint).is_ok(),
                "an encrypted endpoint may point into private space: {endpoint}"
            );
        }
    });
}

#[test]
fn kafka_refuses_every_blocked_corpus_entry_even_when_opted_in() {
    use crate::net::vectors::{MUST_BLOCK, url_host};
    with_kafka_optin(|| {
        for (addr, why) in MUST_BLOCK {
            // Loopback is the one thing the opt-in exists to permit.
            if crate::net::is_loopback_host(addr) {
                continue;
            }
            let endpoint = format!("kafka://{}:9092", url_host(addr));
            assert!(guard_kafka_endpoint(&endpoint).is_err(), "must refuse {addr} ({why})");
            // And again hidden behind a legitimate broker.
            let endpoint = format!("kafka://localhost:9092,{}:9092", url_host(addr));
            assert!(
                guard_kafka_endpoint(&endpoint).is_err(),
                "must refuse {addr} ({why}) even in second position"
            );
        }
    });
}

// ── Kafka SASL resolution ─────────────────────────────────────────────────────

/// Run `f` with the three SASL env vars set to the given values.
fn with_sasl_env<T>(
    mechanism: Option<&str>,
    creds: Option<(&str, &str)>,
    f: impl FnOnce() -> T,
) -> T {
    let (user, pass) = creds.map_or((None, None), |(u, p)| (Some(u), Some(p)));
    temp_env::with_vars(
        [
            ("FRAISEQL_KAFKA_SASL_MECHANISM", mechanism),
            ("FRAISEQL_KAFKA_SASL_USERNAME", user),
            ("FRAISEQL_KAFKA_SASL_PASSWORD", pass),
        ],
        f,
    )
}

#[test]
fn sasl_mechanism_is_required_not_defaulted() {
    // librdkafka's default is GSSAPI, which this build cannot perform; leaving it
    // implicit produces a client-creation error advising a librdkafka recompile.
    with_sasl_env(None, Some(("u", "p")), || {
        assert!(resolve_kafka_sasl().is_err());
    });
    with_sasl_env(Some("   "), Some(("u", "p")), || {
        assert!(resolve_kafka_sasl().is_err(), "whitespace is not a mechanism");
    });
}

#[test]
fn sasl_accepts_the_three_mechanisms_this_build_supports() {
    for (raw, expected) in [
        ("PLAIN", KafkaSaslMechanism::Plain),
        ("plain", KafkaSaslMechanism::Plain),
        ("SCRAM-SHA-256", KafkaSaslMechanism::ScramSha256),
        ("SCRAM-SHA-512", KafkaSaslMechanism::ScramSha512),
    ] {
        with_sasl_env(Some(raw), Some(("u", "p")), || {
            let resolved = resolve_kafka_sasl().unwrap();
            assert_eq!(resolved.mechanism, expected, "{raw}");
            assert_eq!(resolved.username, "u");
            assert_eq!(resolved.password, "p");
        });
    }
}

#[test]
fn sasl_refuses_kerberos_by_name_rather_than_letting_librdkafka_mislead() {
    // librdkafka's own message here is "recompile librdkafka with libsasl2", which
    // is not the fix in this repo — dropping rdkafka's `sasl` feature was a
    // deliberate call, so the refusal explains that instead.
    for raw in ["GSSAPI", "gssapi", "Kerberos"] {
        with_sasl_env(Some(raw), Some(("u", "p")), || {
            let err = resolve_kafka_sasl().unwrap_err();
            assert!(err.contains("Kerberos"), "should name Kerberos: {err}");
            assert!(err.contains("SCRAM-SHA-512"), "should point at a mechanism that works: {err}");
        });
    }
}

#[test]
fn sasl_refuses_oauthbearer_as_unwired_rather_than_half_wiring_it() {
    with_sasl_env(Some("OAUTHBEARER"), Some(("u", "p")), || {
        assert!(resolve_kafka_sasl().is_err());
    });
}

#[test]
fn sasl_requires_both_credentials() {
    for creds in [None, Some(("u", "")), Some(("", "p"))] {
        with_sasl_env(Some("SCRAM-SHA-512"), creds, || {
            assert!(resolve_kafka_sasl().is_err(), "{creds:?} must not authenticate");
        });
    }
}

#[test]
fn sasl_errors_never_echo_the_password() {
    with_sasl_env(Some("NOPE"), Some(("user", "hunter2")), || {
        let err = resolve_kafka_sasl().unwrap_err();
        assert!(!err.contains("hunter2"), "password leaked into an error: {err}");
    });
    with_sasl_env(Some("PLAIN"), Some(("user", "")), || {
        let err = resolve_kafka_sasl().unwrap_err();
        assert!(!err.contains("hunter2"));
    });
}
