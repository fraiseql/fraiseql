//! Kafka endpoint parsing and its transport guard (#1102).
//!
//! Here rather than in a sink or a transport because **two** callers need it and one
//! decision is what a guard is: `fraiseql-cdc-sinks`' outbox drain and
//! `fraiseql-core`'s subscription transport both reach Kafka, and a second copy of
//! "may this endpoint be plaintext?" is a second answer waiting to drift from the
//! first. That drift is not hypothetical here — #816 shipped an inverted plaintext
//! guard on the NATS transport, and #1102 exists because the subscription transport
//! set no `security.protocol` at all while the CDC sink had a screened one.
//!
//! Nothing in this module names an rdkafka type, so the *refusing* half runs in the
//! cheap always-compiled test leg rather than only where a broker feature is enabled.
//! Errors are plain `String`s for the same reason: this crate sits at the bottom of the
//! dependency graph and must not grow a dependency that would make it unreachable from a
//! leaf crate. Each caller wraps the message in its own error type.

/// The transport security a Kafka endpoint declares.
///
/// librdkafka's own `security.protocol` default is `PLAINTEXT`, so this is always
/// set explicitly from the endpoint scheme rather than left to the client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum KafkaSecurityProtocol {
    /// Unencrypted. Reachable only via the development opt-in.
    Plaintext,
    /// TLS, no SASL authentication.
    Ssl,
    /// TLS plus a SASL mechanism (`PLAIN`, `SCRAM-SHA-*` or `OAUTHBEARER`; the
    /// mechanism and credentials are supplied separately).
    SaslSsl,
}

impl KafkaSecurityProtocol {
    /// The literal librdkafka `security.protocol` value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Plaintext => "plaintext",
            Self::Ssl => "ssl",
            Self::SaslSsl => "sasl_ssl",
        }
    }
}

/// A Kafka endpoint that has passed [`guard_kafka_endpoint`].
///
/// There is no other constructor and the struct is `#[non_exhaustive]`, so a
/// value of this type *is* the proof that the guard ran — `bootstrap_servers`
/// cannot be obtained by skipping it.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct KafkaEndpoint {
    /// The protocol to set on the producer, derived from the scheme.
    pub security_protocol: KafkaSecurityProtocol,
    /// The scheme-stripped, comma-separated `host:port` list for librdkafka.
    pub bootstrap_servers: String,
}

/// Env var that permits an unencrypted `kafka://` endpoint.
///
/// Honoured only when `FRAISEQL_ENV` declares a development environment. Named
/// and parsed like the NATS sink's flag so the two cannot disagree about what the
/// operator asked for.
const KAFKA_ALLOW_PLAINTEXT_ENV: &str = "FRAISEQL_KAFKA_ALLOW_PLAINTEXT";

/// Parse and screen a Kafka endpoint, returning the producer settings it implies.
///
/// `bootstrap.servers` is a comma-separated `host:port` list with **no URL
/// scheme**, so transport security is not expressible in it the way `tls://` is
/// for NATS. This requires a scheme of our own — `kafka+ssl://`,
/// `kafka+sasl-ssl://`, or plaintext `kafka://` — maps it to an explicit
/// `security.protocol`, and strips it before the list reaches librdkafka.
///
/// A **scheme-less endpoint is refused, not defaulted**: librdkafka would take it
/// as `PLAINTEXT`, which is the silent downgrade #816 shipped for NATS.
///
/// Plaintext additionally requires the `FRAISEQL_KAFKA_ALLOW_PLAINTEXT` opt-in *and*
/// a declared development environment. On that path **every** broker in the list
/// is screened through [`crate::net`], not just the first: librdkafka
/// contacts each bootstrap server, so one `169.254.169.254:9092` hiding behind a
/// loopback entry would otherwise turn the dev escape hatch into an SSRF licence.
///
/// Screening is deliberately *not* applied to the encrypted schemes. MSK and
/// every VPC-hosted cluster address brokers in RFC 1918 space, which
/// [`crate::net::blocked_host_reason`] refuses — the guard exists to stop
/// the plaintext escape hatch reaching further than localhost, not to veto where
/// an operator points an encrypted connection.
///
/// # Errors
///
/// Returns a description of the refusal — a missing or unsupported scheme, a
/// malformed or empty broker list, an unopted-in plaintext endpoint, or a blocked
/// broker host on the plaintext path. The caller wraps it in its own error type.
pub fn guard_kafka_endpoint(endpoint: &str) -> Result<KafkaEndpoint, String> {
    let Some((scheme, rest)) = endpoint.split_once("://") else {
        return Err(format!(
            "kafka endpoint {endpoint:?} has no scheme. bootstrap.servers carries no \
             transport security, and librdkafka reads a bare list as PLAINTEXT, so the \
             scheme is required here rather than defaulted: use kafka+ssl://host:port, \
             kafka+sasl-ssl://host:port, or kafka://host:port for a dev broker."
        ));
    };

    let security_protocol = match scheme.to_ascii_lowercase().as_str() {
        "kafka" => KafkaSecurityProtocol::Plaintext,
        "kafka+ssl" => KafkaSecurityProtocol::Ssl,
        "kafka+sasl-ssl" => KafkaSecurityProtocol::SaslSsl,
        other => {
            return Err(format!(
                "unsupported kafka endpoint scheme {other:?} in {endpoint:?}: use \
                 kafka+ssl://, kafka+sasl-ssl://, or kafka:// (plaintext, dev only)."
            ));
        },
    };

    let brokers = parse_broker_list(rest, endpoint)?;

    if security_protocol == KafkaSecurityProtocol::Plaintext {
        if !crate::deployment::insecure_bypass_allowed(crate::deployment::env_opt_in(
            KAFKA_ALLOW_PLAINTEXT_ENV,
        )) {
            return Err(format!(
                "refusing plaintext kafka:// endpoint {endpoint:?}: change events would \
                 cross the wire in the clear. Use kafka+ssl:// or kafka+sasl-ssl://, or \
                 set {KAFKA_ALLOW_PLAINTEXT_ENV}=true with FRAISEQL_ENV=development for \
                 dev/CI."
            ));
        }

        // Opted in and in development. The opt-in exists to reach a broker on
        // localhost; it must not also unlock the instance-metadata service or an
        // internal network. Every entry is checked — librdkafka contacts them all.
        for broker in &brokers {
            let host = crate::net::host_of_authority(broker);
            if crate::net::is_loopback_host(host) {
                continue;
            }
            if let Some(reason) = crate::net::blocked_host_reason(host) {
                return Err(format!(
                    "refusing kafka endpoint {endpoint:?}: bootstrap server {broker:?} \
                     is not permitted ({reason}). The plaintext opt-in reaches loopback \
                     brokers only."
                ));
            }
        }
    }

    Ok(KafkaEndpoint {
        security_protocol,
        bootstrap_servers: brokers.join(","),
    })
}

/// Split and validate the scheme-stripped broker list.
///
/// Entries are trimmed of surrounding whitespace. An empty entry is an error
/// rather than a silent drop — a trailing comma must not quietly shrink the
/// bootstrap set. Userinfo and path/query/fragment components are refused
/// outright: none is legal in `bootstrap.servers`, and accepting them would let a
/// host be masked the way `nats://user:pw@host` masked one before #816.
fn parse_broker_list(rest: &str, endpoint: &str) -> Result<Vec<String>, String> {
    let mut brokers = Vec::new();
    for raw in rest.split(',') {
        let entry = raw.trim();
        if entry.is_empty() {
            return Err(format!(
                "kafka endpoint {endpoint:?} has an empty bootstrap server entry; an \
                 empty entry is refused rather than dropped so the broker list cannot \
                 silently shrink."
            ));
        }
        if let Some(bad) = entry.chars().find(|c| matches!(c, '@' | '/' | '?' | '#')) {
            return Err(format!(
                "kafka bootstrap server {entry:?} in {endpoint:?} contains {bad:?}. \
                 bootstrap.servers accepts host:port only — userinfo and path \
                 components are not valid there and can mask the real host."
            ));
        }
        if crate::net::host_of_authority(entry).is_empty() {
            return Err(format!("kafka bootstrap server {entry:?} in {endpoint:?} has no host."));
        }
        brokers.push(entry.to_owned());
    }
    Ok(brokers)
}

/// A SASL mechanism this build can actually perform.
///
/// Deliberately not the full Kafka set. rdkafka's `sasl` feature — which is what
/// pulls Cyrus libsasl2 and with it `GSSAPI`/Kerberos — is not enabled, because it
/// is not additive: `sasl` implies `ssl` and adds nothing but Kerberos, at the
/// cost of a hard build-time and runtime dependency on libsasl2. `ssl` alone
/// compiles `PLAIN`, `SCRAM-SHA-*` and `OAUTHBEARER`, which is every mechanism a
/// managed Kafka offers. librdkafka reports the same list itself when asked for
/// `GSSAPI`: *"Current build options: PLAIN `SASL_SCRAM` OAUTHBEARER"*.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum KafkaSaslMechanism {
    /// Username/password in the clear *inside* the TLS session (Confluent Cloud).
    Plain,
    /// SCRAM-SHA-256 (Redpanda Cloud's default).
    ScramSha256,
    /// SCRAM-SHA-512 (AWS MSK's default).
    ScramSha512,
}

impl KafkaSaslMechanism {
    /// The literal librdkafka `sasl.mechanism` value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Plain => "PLAIN",
            Self::ScramSha256 => "SCRAM-SHA-256",
            Self::ScramSha512 => "SCRAM-SHA-512",
        }
    }
}

/// Resolved SASL settings for a `kafka+sasl-ssl://` endpoint.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct KafkaSaslCredentials {
    /// The mechanism to authenticate with.
    pub mechanism: KafkaSaslMechanism,
    /// The SASL username.
    pub username:  String,
    /// The SASL password.
    pub password:  String,
}

/// Env var naming the SASL mechanism for a `kafka+sasl-ssl://` endpoint.
const KAFKA_SASL_MECHANISM_ENV: &str = "FRAISEQL_KAFKA_SASL_MECHANISM";
/// Env var carrying the SASL username.
const KAFKA_SASL_USERNAME_ENV: &str = "FRAISEQL_KAFKA_SASL_USERNAME";
/// Env var carrying the SASL password.
const KAFKA_SASL_PASSWORD_ENV: &str = "FRAISEQL_KAFKA_SASL_PASSWORD";

/// Resolve the SASL mechanism and credentials for a `kafka+sasl-ssl://` endpoint.
///
/// Credentials come from the environment rather than the sink config because they
/// are secrets and the config is destined for a TOML surface.
///
/// The mechanism is **required, not defaulted**. librdkafka's own default is
/// `GSSAPI`, which this build cannot perform at all — leaving it implicit means
/// `kafka+sasl-ssl://` fails at client creation with advice to *"recompile
/// librdkafka with libsasl2"*, which is both confusing and the wrong fix here.
/// There is also no defensible default among the three supported mechanisms:
/// Confluent Cloud wants `PLAIN`, MSK `SCRAM-SHA-512`, Redpanda `SCRAM-SHA-256`.
///
/// # Errors
///
/// Returns a description of the refusal if the mechanism is unset, unsupported by
/// this build, or if credentials are missing. Never echoes the password.
pub fn resolve_kafka_sasl() -> Result<KafkaSaslCredentials, String> {
    let raw = std::env::var(KAFKA_SASL_MECHANISM_ENV).unwrap_or_default();
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(format!(
            "kafka+sasl-ssl:// requires {KAFKA_SASL_MECHANISM_ENV}. librdkafka would \
             otherwise default to GSSAPI, which this build cannot perform, and no default \
             fits every broker (Confluent Cloud uses PLAIN, MSK SCRAM-SHA-512, Redpanda \
             SCRAM-SHA-256). Set it to PLAIN, SCRAM-SHA-256 or SCRAM-SHA-512."
        ));
    }

    let mechanism = match raw.to_ascii_uppercase().as_str() {
        "PLAIN" => KafkaSaslMechanism::Plain,
        "SCRAM-SHA-256" => KafkaSaslMechanism::ScramSha256,
        "SCRAM-SHA-512" => KafkaSaslMechanism::ScramSha512,
        "GSSAPI" | "KERBEROS" => {
            return Err(format!(
                "SASL mechanism {raw:?} is not available: this build links no Cyrus \
                 libsasl2, so Kerberos cannot be performed. librdkafka's own error here \
                 suggests recompiling it, which is not the fix — enabling rdkafka's `sasl` \
                 feature would add a hard libsasl2 build and runtime dependency for \
                 Kerberos alone. Use PLAIN, SCRAM-SHA-256 or SCRAM-SHA-512."
            ));
        },
        "OAUTHBEARER" => {
            return Err(
                "SASL mechanism \"OAUTHBEARER\" is compiled in but not wired up: it needs \
                 a token-refresh callback, and a producer that never refreshes its token \
                 fails once the first one expires. Refused by name rather than half-wired. \
                 Use PLAIN, SCRAM-SHA-256 or SCRAM-SHA-512."
                    .to_owned(),
            );
        },
        other => {
            return Err(format!(
                "unsupported SASL mechanism {other:?}: use PLAIN, SCRAM-SHA-256 or \
                 SCRAM-SHA-512."
            ));
        },
    };

    let username = std::env::var(KAFKA_SASL_USERNAME_ENV).unwrap_or_default();
    let password = std::env::var(KAFKA_SASL_PASSWORD_ENV).unwrap_or_default();
    if username.is_empty() || password.is_empty() {
        return Err(format!(
            "SASL mechanism {} requires both {KAFKA_SASL_USERNAME_ENV} and \
             {KAFKA_SASL_PASSWORD_ENV}.",
            mechanism.as_str()
        ));
    }

    Ok(KafkaSaslCredentials {
        mechanism,
        username,
        password,
    })
}

#[cfg(test)]
mod tests;
