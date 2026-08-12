//! The broker-agnostic sink trait, routing config, and pure encoding helpers.
//!
//! Everything here is **always compiled** (no broker feature), so the subject
//! sanitiser, the per-tenant/per-table filter, and the backoff schedule are
//! exercised by the fast unit-test leg, not only behind a broker feature.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{error::CdcError, event::ChangeEvent};

/// Which broker a sink targets. Serialised `kebab-case` so TOML `kind =
/// "nats-jetstream"` round-trips.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum SinkKind {
    /// Apache Kafka (feature `cdc-kafka`).
    Kafka,
    /// NATS `JetStream`. Renamed explicitly so the TOML `kind` is the compact
    /// `"nats-jetstream"` rather than kebab-case's `"nats-jet-stream"`.
    #[serde(rename = "nats-jetstream")]
    NatsJetStream,
    /// AWS Kinesis (not yet implemented).
    Kinesis,
    /// Apache Pulsar (not yet implemented).
    Pulsar,
}

/// The outcome of publishing one event to a broker.
///
/// Mirrors the observer transient-vs-permanent classification
/// (`fraiseql-observers/src/actions.rs`): a transient failure is retried with
/// backoff; a permanent failure goes straight to the dead-letter state (`dead`).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PublishOutcome {
    /// The broker acknowledged the record.
    Published,
    /// A retryable failure (broker down, ack timeout) — retry with backoff.
    Transient(String),
    /// A non-retryable failure (un-renderable subject, encode error) — dead-letter.
    Permanent(String),
}

/// Default delivery attempt ceiling before a tracking row is dead-lettered.
const fn default_max_attempts() -> i32 {
    8
}

/// Routing + filtering configuration for a single sink.
///
/// Serde-ready for a future `[[cdc.outbound.sinks]]` TOML surface (server
/// auto-mount is deferred — see the slice plan).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct CdcSinkConfig {
    /// Stable sink identifier (the per-sink delivery-state partition key).
    pub name:             String,
    /// Subject/topic template, e.g. `fraiseql.{tenant_id}.{table}`. Supported
    /// placeholders: `{tenant_id}`, `{table}`, `{op}`.
    pub subject_template: String,
    /// Optional table allow-list (`object_type`); `None` ⇒ all tables match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tables:           Option<Vec<String>>,
    /// Optional tenant allow-list; `None` ⇒ all tenants match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenants:          Option<Vec<Uuid>>,
    /// Delivery attempts before a tracking row is dead-lettered.
    #[serde(default = "default_max_attempts")]
    pub max_attempts:     i32,
}

impl CdcSinkConfig {
    /// Construct a config with the default `max_attempts` and no filters.
    #[must_use]
    pub fn new(name: impl Into<String>, subject_template: impl Into<String>) -> Self {
        Self {
            name:             name.into(),
            subject_template: subject_template.into(),
            tables:           None,
            tenants:          None,
            max_attempts:     default_max_attempts(),
        }
    }

    /// Restrict this sink to a set of tables (`object_type`).
    #[must_use]
    pub fn with_tables(mut self, tables: Vec<String>) -> Self {
        self.tables = Some(tables);
        self
    }

    /// Restrict this sink to a set of tenants.
    #[must_use]
    pub fn with_tenants(mut self, tenants: Vec<Uuid>) -> Self {
        self.tenants = Some(tenants);
        self
    }

    /// Whether an event passes this sink's per-tenant + per-table filter.
    ///
    /// This is the pure mirror of the SQL `enqueue` predicate — a `None`
    /// allow-list matches everything; a tenant allow-list rejects events with no
    /// tenant stamp.
    #[must_use]
    pub fn matches(&self, ev: &ChangeEvent) -> bool {
        if let Some(tables) = &self.tables {
            if !tables.iter().any(|t| t == &ev.object_type) {
                return false;
            }
        }
        if let Some(tenants) = &self.tenants {
            match ev.tenant_id {
                Some(tid) if tenants.contains(&tid) => {},
                _ => return false,
            }
        }
        true
    }
}

/// A broker sink that publishes change events.
///
/// Contrast with `fraiseql_core::runtime::subscription::TransportAdapter`
/// (fire-and-forget; an event is lost if the producer call fails): a `CdcSink`
/// is driven by the durable [`crate::DrainWorker`], so a broker outage produces
/// retry/backlog, never loss.
///
/// `publish` is declared with an explicit `impl Future + Send` return type
/// (RPITIT) rather than `async fn`, so no `async_trait` macro is introduced and
/// the returned future is spawnable.
pub trait CdcSink {
    /// The sink's stable name (matches its `CdcSinkConfig::name`).
    fn name(&self) -> &str;

    /// Which broker this sink targets.
    fn kind(&self) -> SinkKind;

    /// Whether this sink should receive the given event.
    fn matches(&self, ev: &ChangeEvent) -> bool;

    /// Publish one event, returning the delivery outcome.
    ///
    /// Implementations must never panic; transport failures are reported as
    /// [`PublishOutcome::Transient`] (retryable) or [`PublishOutcome::Permanent`]
    /// (dead-letter), never via the return type's error channel (there is none).
    fn publish(&self, ev: &ChangeEvent)
    -> impl std::future::Future<Output = PublishOutcome> + Send;
}

/// Render a subject/topic template against an event, sanitising every
/// interpolated value for the NATS subject charset.
///
/// Supported placeholders: `{tenant_id}` (a `None` tenant renders as `_none_`),
/// `{table}`, `{op}`. Returns `Err` — which the caller treats as a *permanent*
/// failure (dead-letter), never a silent re-route — if any interpolated value
/// contains a NATS-illegal character (`.`, `*`, `>`, whitespace, or control),
/// which would otherwise let a crafted tenant key escape into another subject
/// namespace (the topic-injection risk, R2).
///
/// # Errors
///
/// Returns the offending segment description if a value is empty or contains an
/// illegal character.
// Reason: the `{tenant_id}`/`{table}`/`{op}` literals are subject-template
// placeholders matched by `str::replace`, not `format!` arguments.
#[allow(clippy::literal_string_with_formatting_args)]
pub fn render_subject(template: &str, ev: &ChangeEvent) -> Result<String, String> {
    let tenant = ev.tenant_id.map_or_else(|| "_none_".to_owned(), |t| t.to_string());
    let table = sanitize_segment(&ev.object_type)?;
    let tenant = sanitize_segment(&tenant)?;
    let op = sanitize_segment(ev.op.as_str())?;
    Ok(template
        .replace("{tenant_id}", &tenant)
        .replace("{table}", &table)
        .replace("{op}", &op))
}

/// Validate a single interpolated subject segment against the NATS charset.
fn sanitize_segment(segment: &str) -> Result<String, String> {
    if segment.is_empty() {
        return Err("empty subject segment".to_owned());
    }
    for c in segment.chars() {
        if c == '.' || c == '*' || c == '>' || c.is_whitespace() || c.is_control() {
            return Err(format!(
                "subject segment {segment:?} contains NATS-illegal character {c:?}"
            ));
        }
    }
    Ok(segment.to_owned())
}

/// Capped exponential backoff for a 1-based retry attempt: attempt 1 → 1s,
/// 2 → 2s, 3 → 4s, … capped at 5 minutes.
#[must_use]
pub fn next_attempt_delay(attempt: u32) -> Duration {
    let secs = 1u64.checked_shl(attempt.saturating_sub(1)).unwrap_or(u64::MAX).min(300);
    Duration::from_secs(secs)
}

// ── Kafka endpoint parsing + transport guard ─────────────────────────────────
//
// Pure, and therefore here rather than in the feature-gated `kafka` module: no
// rdkafka type appears in these signatures, so the *refusing* half of the guard
// is exercised by the always-compiled unit-test leg instead of only where
// `cdc-kafka` is enabled. The NATS guard lives behind `cdc-nats-jetstream` and is
// only ever run there — worth not repeating.

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

/// Kafka's own topic-name length cap.
const KAFKA_TOPIC_MAX_LEN: usize = 249;

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
/// is screened through [`fraiseql_guard::net`], not just the first: librdkafka
/// contacts each bootstrap server, so one `169.254.169.254:9092` hiding behind a
/// loopback entry would otherwise turn the dev escape hatch into an SSRF licence.
///
/// Screening is deliberately *not* applied to the encrypted schemes. MSK and
/// every VPC-hosted cluster address brokers in RFC 1918 space, which
/// [`fraiseql_guard::net::blocked_host_reason`] refuses — the guard exists to stop
/// the plaintext escape hatch reaching further than localhost, not to veto where
/// an operator points an encrypted connection.
///
/// # Errors
///
/// Returns [`CdcError::Config`] for a missing or unsupported scheme, a malformed
/// or empty broker list, an unopted-in plaintext endpoint, or any blocked broker
/// host on the plaintext path.
pub fn guard_kafka_endpoint(endpoint: &str) -> crate::error::Result<KafkaEndpoint> {
    let Some((scheme, rest)) = endpoint.split_once("://") else {
        return Err(CdcError::Config(format!(
            "kafka endpoint {endpoint:?} has no scheme. bootstrap.servers carries no \
             transport security, and librdkafka reads a bare list as PLAINTEXT, so the \
             scheme is required here rather than defaulted: use kafka+ssl://host:port, \
             kafka+sasl-ssl://host:port, or kafka://host:port for a dev broker."
        )));
    };

    let security_protocol = match scheme.to_ascii_lowercase().as_str() {
        "kafka" => KafkaSecurityProtocol::Plaintext,
        "kafka+ssl" => KafkaSecurityProtocol::Ssl,
        "kafka+sasl-ssl" => KafkaSecurityProtocol::SaslSsl,
        other => {
            return Err(CdcError::Config(format!(
                "unsupported kafka endpoint scheme {other:?} in {endpoint:?}: use \
                 kafka+ssl://, kafka+sasl-ssl://, or kafka:// (plaintext, dev only)."
            )));
        },
    };

    let brokers = parse_broker_list(rest, endpoint)?;

    if security_protocol == KafkaSecurityProtocol::Plaintext {
        if !fraiseql_guard::deployment::insecure_bypass_allowed(
            fraiseql_guard::deployment::env_opt_in(KAFKA_ALLOW_PLAINTEXT_ENV),
        ) {
            return Err(CdcError::Config(format!(
                "refusing plaintext kafka:// endpoint {endpoint:?}: change events would \
                 cross the wire in the clear. Use kafka+ssl:// or kafka+sasl-ssl://, or \
                 set {KAFKA_ALLOW_PLAINTEXT_ENV}=true with FRAISEQL_ENV=development for \
                 dev/CI."
            )));
        }

        // Opted in and in development. The opt-in exists to reach a broker on
        // localhost; it must not also unlock the instance-metadata service or an
        // internal network. Every entry is checked — librdkafka contacts them all.
        for broker in &brokers {
            let host = broker_host(broker);
            if fraiseql_guard::net::is_loopback_host(host) {
                continue;
            }
            if let Some(reason) = fraiseql_guard::net::blocked_host_reason(host) {
                return Err(CdcError::Config(format!(
                    "refusing kafka endpoint {endpoint:?}: bootstrap server {broker:?} \
                     is not permitted ({reason}). The plaintext opt-in reaches loopback \
                     brokers only."
                )));
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
fn parse_broker_list(rest: &str, endpoint: &str) -> crate::error::Result<Vec<String>> {
    let mut brokers = Vec::new();
    for raw in rest.split(',') {
        let entry = raw.trim();
        if entry.is_empty() {
            return Err(CdcError::Config(format!(
                "kafka endpoint {endpoint:?} has an empty bootstrap server entry; an \
                 empty entry is refused rather than dropped so the broker list cannot \
                 silently shrink."
            )));
        }
        if let Some(bad) = entry.chars().find(|c| matches!(c, '@' | '/' | '?' | '#')) {
            return Err(CdcError::Config(format!(
                "kafka bootstrap server {entry:?} in {endpoint:?} contains {bad:?}. \
                 bootstrap.servers accepts host:port only — userinfo and path \
                 components are not valid there and can mask the real host."
            )));
        }
        if broker_host(entry).is_empty() {
            return Err(CdcError::Config(format!(
                "kafka bootstrap server {entry:?} in {endpoint:?} has no host."
            )));
        }
        brokers.push(entry.to_owned());
    }
    Ok(brokers)
}

/// Extract the host from a `host:port` bootstrap entry.
///
/// `IPv6` literals keep their brackets, which both
/// [`fraiseql_guard::net::is_loopback_host`] and
/// [`fraiseql_guard::net::blocked_host_reason`] handle.
fn broker_host(entry: &str) -> &str {
    if entry.starts_with('[') {
        if let Some(end) = entry.find(']') {
            return &entry[..=end];
        }
    }
    entry.split(':').next().unwrap_or(entry)
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
/// Returns [`CdcError::Config`] if the mechanism is unset, unsupported by this
/// build, or if credentials are missing. Never echoes the password.
pub fn resolve_kafka_sasl() -> crate::error::Result<KafkaSaslCredentials> {
    let raw = std::env::var(KAFKA_SASL_MECHANISM_ENV).unwrap_or_default();
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(CdcError::Config(format!(
            "kafka+sasl-ssl:// requires {KAFKA_SASL_MECHANISM_ENV}. librdkafka would \
             otherwise default to GSSAPI, which this build cannot perform, and no default \
             fits every broker (Confluent Cloud uses PLAIN, MSK SCRAM-SHA-512, Redpanda \
             SCRAM-SHA-256). Set it to PLAIN, SCRAM-SHA-256 or SCRAM-SHA-512."
        )));
    }

    let mechanism = match raw.to_ascii_uppercase().as_str() {
        "PLAIN" => KafkaSaslMechanism::Plain,
        "SCRAM-SHA-256" => KafkaSaslMechanism::ScramSha256,
        "SCRAM-SHA-512" => KafkaSaslMechanism::ScramSha512,
        "GSSAPI" | "KERBEROS" => {
            return Err(CdcError::Config(format!(
                "SASL mechanism {raw:?} is not available: this build links no Cyrus \
                 libsasl2, so Kerberos cannot be performed. librdkafka's own error here \
                 suggests recompiling it, which is not the fix — enabling rdkafka's `sasl` \
                 feature would add a hard libsasl2 build and runtime dependency for \
                 Kerberos alone. Use PLAIN, SCRAM-SHA-256 or SCRAM-SHA-512."
            )));
        },
        "OAUTHBEARER" => {
            return Err(CdcError::Config(
                "SASL mechanism \"OAUTHBEARER\" is compiled in but not wired up: it needs \
                 a token-refresh callback, and a producer that never refreshes its token \
                 fails once the first one expires. Refused by name rather than half-wired. \
                 Use PLAIN, SCRAM-SHA-256 or SCRAM-SHA-512."
                    .to_owned(),
            ));
        },
        other => {
            return Err(CdcError::Config(format!(
                "unsupported SASL mechanism {other:?}: use PLAIN, SCRAM-SHA-256 or \
                 SCRAM-SHA-512."
            )));
        },
    };

    let username = std::env::var(KAFKA_SASL_USERNAME_ENV).unwrap_or_default();
    let password = std::env::var(KAFKA_SASL_PASSWORD_ENV).unwrap_or_default();
    if username.is_empty() || password.is_empty() {
        return Err(CdcError::Config(format!(
            "SASL mechanism {} requires both {KAFKA_SASL_USERNAME_ENV} and \
             {KAFKA_SASL_PASSWORD_ENV}.",
            mechanism.as_str()
        )));
    }

    Ok(KafkaSaslCredentials {
        mechanism,
        username,
        password,
    })
}

/// Validate a rendered topic against Kafka's own topic-name rules.
///
/// Kafka permits `[a-zA-Z0-9._-]` only, caps names at 249 characters, and
/// reserves `.` and `..`. This is a *narrower* charset than a NATS subject, so a
/// template that renders legally for the NATS sink can be illegal here — hence a
/// separate check rather than reuse of [`render_subject`]'s sanitiser alone.
///
/// # Errors
///
/// Returns a description of the violation, which the caller reports as
/// [`PublishOutcome::Permanent`] (dead-letter) — never a silent re-route to a
/// different topic.
pub fn validate_kafka_topic(topic: &str) -> Result<(), String> {
    if topic.is_empty() {
        return Err("kafka topic is empty".to_owned());
    }
    if topic == "." || topic == ".." {
        return Err(format!("kafka reserves the topic name {topic:?}"));
    }
    if topic.chars().count() > KAFKA_TOPIC_MAX_LEN {
        return Err(format!(
            "kafka topic is {} characters, over the {KAFKA_TOPIC_MAX_LEN}-character cap",
            topic.chars().count()
        ));
    }
    for c in topic.chars() {
        if !(c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-') {
            return Err(format!(
                "kafka topic {topic:?} contains illegal character {c:?}; Kafka allows \
                 [a-zA-Z0-9._-] only"
            ));
        }
    }
    Ok(())
}

/// The partition key for an event: the changed entity's identity.
///
/// Shared by every sink whose broker partitions by a hashed key — Kafka hashes it
/// to choose a partition, Kinesis MD5-hashes it to choose a shard — and both order
/// records only *within* one of those. Keying by anything unique per message (a
/// `seq`, say) would therefore scatter one entity's changes across every partition
/// and destroy the ordering the idempotent producer exists to provide. Consumer
/// dedup is served separately by the `(object_type, seq)` pair, which travels in
/// the payload.
///
/// Falls back to the object type alone when a row carries no `object_id`, which
/// keeps the key deterministic (never null, never random) and degrades to
/// per-table rather than per-entity ordering.
///
/// The result is bounded well under Kinesis's 256-character partition-key limit: a
/// PostgreSQL identifier is at most 63 bytes, plus a separator and a 36-character
/// UUID.
#[must_use]
pub fn entity_partition_key(ev: &ChangeEvent) -> String {
    ev.object_id
        .map_or_else(|| ev.object_type.clone(), |id| format!("{}:{}", ev.object_type, id))
}

// ── Kinesis endpoint parsing + transport guard ───────────────────────────────
//
// Pure, and here rather than in the feature-gated `kinesis` module for the same
// reason the Kafka guard is: no aws-sdk type appears in these signatures, so the
// refusing half runs in the always-compiled unit-test leg.

/// Env var that permits an unencrypted `http://` Kinesis endpoint override.
///
/// Honoured only when `FRAISEQL_ENV` declares a development environment, like the
/// Kafka and NATS flags it is named after.
const KINESIS_ALLOW_PLAINTEXT_ENV: &str = "FRAISEQL_KINESIS_ALLOW_PLAINTEXT";

/// Env var carrying an endpoint-URL override (`LocalStack`, a VPC interface
/// endpoint). Absent means the SDK resolves the real regional endpoint.
const KINESIS_ENDPOINT_URL_ENV: &str = "FRAISEQL_KINESIS_ENDPOINT_URL";

/// Kinesis's own stream-name length cap.
const KINESIS_STREAM_MAX_LEN: usize = 128;

/// Longest region identifier accepted. AWS's longest today is well under this.
const KINESIS_REGION_MAX_LEN: usize = 64;

/// A Kinesis endpoint that has passed [`guard_kinesis_endpoint`].
///
/// There is no other constructor and the struct is `#[non_exhaustive]`, so a value
/// of this type *is* the proof that the guard ran.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct KinesisEndpoint {
    /// The AWS region, lowercased.
    pub region: String,
}

/// Parse and screen a Kinesis endpoint, returning the region it names.
///
/// Unlike Kafka, Kinesis is not addressed by a broker list: the AWS SDK resolves a
/// regional HTTPS endpoint from a region name. The configured endpoint therefore
/// carries **only** the region, in the form `kinesis://<region>`. A scheme-less
/// value is refused rather than taken as a bare region — it keeps the sink kind
/// unambiguous in `[cdc_outbound]` and matches the contract
/// [`guard_kafka_endpoint`] states.
///
/// The region is constrained to `[a-z0-9-]` starting with a letter because it is
/// interpolated into the endpoint the SDK resolves; an unconstrained value is an
/// injection seam, not merely a typo. It is lowercased on the way through, since
/// AWS region identifiers are canonically lowercase and the alternative is an
/// opaque signature failure from the far end.
///
/// Transport security is *not* expressed here: absent an override the SDK talks
/// HTTPS to AWS. The escape hatch is [`resolve_kinesis_endpoint_url`], which is
/// where the plaintext guard lives.
///
/// # Errors
///
/// Returns [`CdcError::Config`] for a missing or unsupported scheme, an empty
/// region, a region carrying userinfo/path/query components, or one outside the
/// permitted charset.
pub fn guard_kinesis_endpoint(endpoint: &str) -> crate::error::Result<KinesisEndpoint> {
    let Some((scheme, rest)) = endpoint.split_once("://") else {
        return Err(CdcError::Config(format!(
            "kinesis endpoint {endpoint:?} has no scheme: use kinesis://<region>, e.g. \
             kinesis://us-east-1. The scheme is required rather than defaulted so the sink \
             kind stays unambiguous."
        )));
    };

    if !scheme.eq_ignore_ascii_case("kinesis") {
        return Err(CdcError::Config(format!(
            "unsupported kinesis endpoint scheme {scheme:?} in {endpoint:?}: use \
             kinesis://<region>. The transport is chosen by the AWS SDK (HTTPS) or by \
             {KINESIS_ENDPOINT_URL_ENV}, not by this scheme."
        )));
    }

    if let Some(bad) = rest.chars().find(|c| matches!(c, '@' | '/' | '?' | '#')) {
        return Err(CdcError::Config(format!(
            "kinesis endpoint {endpoint:?} contains {bad:?}. A kinesis:// endpoint carries \
             an AWS region and nothing else — userinfo, path and query components are not \
             valid there and can mask the region actually used."
        )));
    }

    let region = rest.to_ascii_lowercase();
    validate_aws_region(&region, endpoint)?;
    Ok(KinesisEndpoint { region })
}

/// Validate an already-lowercased AWS region identifier.
fn validate_aws_region(region: &str, endpoint: &str) -> crate::error::Result<()> {
    if region.is_empty() {
        return Err(CdcError::Config(format!(
            "kinesis endpoint {endpoint:?} names no region: use kinesis://<region>."
        )));
    }
    if region.len() > KINESIS_REGION_MAX_LEN {
        return Err(CdcError::Config(format!(
            "kinesis region in {endpoint:?} is {} characters, over the \
             {KINESIS_REGION_MAX_LEN}-character cap",
            region.len()
        )));
    }
    if !region.starts_with(|c: char| c.is_ascii_lowercase()) {
        return Err(CdcError::Config(format!(
            "kinesis region in {endpoint:?} must start with a letter; AWS regions look like \
             us-east-1 or eu-west-3."
        )));
    }
    if let Some(bad) = region.chars().find(|c| !(c.is_ascii_alphanumeric() || *c == '-')) {
        return Err(CdcError::Config(format!(
            "kinesis region in {endpoint:?} contains illegal character {bad:?}; an AWS region \
             is [a-z0-9-] only, e.g. us-east-1."
        )));
    }
    Ok(())
}

/// Resolve the optional endpoint-URL override, screening it when unencrypted.
///
/// Absent (or blank) means **no override**: the SDK resolves the real regional
/// endpoint, which is HTTPS. That is the production shape and is not an error.
///
/// An `https://` override is accepted as given. Screening is deliberately not
/// applied to it, for the reason it is not applied to `kafka+ssl://`: a VPC
/// interface endpoint for Kinesis resolves into RFC 1918 space, which
/// [`fraiseql_guard::net::blocked_host_reason`] refuses, and the guard exists to
/// stop the *plaintext* escape hatch reaching further than localhost — not to veto
/// where an operator points an encrypted connection.
///
/// An `http://` override carries the full row after-image of every mutation in the
/// clear, so it requires the `FRAISEQL_KINESIS_ALLOW_PLAINTEXT` opt-in *and* a
/// declared development environment, *and* a loopback host. That combination is
/// what a `LocalStack` container needs and nothing more; without the host check the
/// dev escape hatch would double as an SSRF licence into the instance-metadata
/// service.
///
/// # Errors
///
/// Returns [`CdcError::Config`] for a missing or unsupported scheme, a URL
/// carrying userinfo/path/query components, an unopted-in `http://` override, or a
/// non-loopback host on the plaintext path.
pub fn resolve_kinesis_endpoint_url() -> crate::error::Result<Option<String>> {
    let raw = std::env::var(KINESIS_ENDPOINT_URL_ENV).unwrap_or_default();
    let url = raw.trim();
    if url.is_empty() {
        return Ok(None);
    }

    let Some((scheme, rest)) = url.split_once("://") else {
        return Err(CdcError::Config(format!(
            "{KINESIS_ENDPOINT_URL_ENV}={url:?} has no scheme: use https://host[:port], or \
             http://localhost:port for a dev endpoint under \
             {KINESIS_ALLOW_PLAINTEXT_ENV}=true."
        )));
    };

    let plaintext = match scheme.to_ascii_lowercase().as_str() {
        "https" => false,
        "http" => true,
        other => {
            return Err(CdcError::Config(format!(
                "unsupported scheme {other:?} in {KINESIS_ENDPOINT_URL_ENV}={url:?}: the \
                 Kinesis API speaks HTTPS; use https://, or http:// for a dev endpoint."
            )));
        },
    };

    let authority = rest.strip_suffix('/').unwrap_or(rest);
    if let Some(bad) = authority.chars().find(|c| matches!(c, '@' | '/' | '?' | '#')) {
        return Err(CdcError::Config(format!(
            "{KINESIS_ENDPOINT_URL_ENV}={url:?} contains {bad:?}. An endpoint override is a \
             host and optional port — userinfo and path components are not valid there and \
             can mask the real host."
        )));
    }
    let host = broker_host(authority);
    if host.is_empty() {
        return Err(CdcError::Config(format!("{KINESIS_ENDPOINT_URL_ENV}={url:?} has no host.")));
    }

    if plaintext {
        if !fraiseql_guard::deployment::insecure_bypass_allowed(
            fraiseql_guard::deployment::env_opt_in(KINESIS_ALLOW_PLAINTEXT_ENV),
        ) {
            return Err(CdcError::Config(format!(
                "refusing plaintext {KINESIS_ENDPOINT_URL_ENV}={url:?}: change events would \
                 cross the wire in the clear. Use https://, or set \
                 {KINESIS_ALLOW_PLAINTEXT_ENV}=true with FRAISEQL_ENV=development for \
                 dev/CI against LocalStack."
            )));
        }
        // Loopback is what the opt-in exists for. Any other host is screened —
        // identically to the Kafka guard's plaintext path, so the two cannot
        // disagree about what the operator asked for. A CI bind hostname
        // (`localstack`) resolves to none of the blocked classes and is admitted;
        // the instance-metadata address and RFC 1918 space are not.
        if !fraiseql_guard::net::is_loopback_host(host) {
            if let Some(reason) = fraiseql_guard::net::blocked_host_reason(host) {
                return Err(CdcError::Config(format!(
                    "refusing {KINESIS_ENDPOINT_URL_ENV}={url:?}: host {host:?} is not \
                     permitted ({reason}). The plaintext opt-in reaches a dev endpoint, not \
                     the instance-metadata service or an internal network."
                )));
            }
        }
    }

    Ok(Some(url.to_owned()))
}

/// Validate a rendered stream name against Kinesis's own rules.
///
/// Kinesis permits `[a-zA-Z0-9_.-]` and caps names at 128 characters — a
/// *narrower* cap than Kafka's 249, so a template that renders legally for the
/// Kafka sink can be illegal here. Hence a separate check rather than reuse of
/// [`validate_kafka_topic`].
///
/// # Errors
///
/// Returns a description of the violation, which the caller reports as
/// [`PublishOutcome::Permanent`] (dead-letter) — never a silent re-route to a
/// different stream.
pub fn validate_kinesis_stream(stream: &str) -> Result<(), String> {
    if stream.is_empty() {
        return Err("kinesis stream name is empty".to_owned());
    }
    if stream.chars().count() > KINESIS_STREAM_MAX_LEN {
        return Err(format!(
            "kinesis stream name is {} characters, over the \
             {KINESIS_STREAM_MAX_LEN}-character cap",
            stream.chars().count()
        ));
    }
    for c in stream.chars() {
        if !(c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-') {
            return Err(format!(
                "kinesis stream name {stream:?} contains illegal character {c:?}; Kinesis \
                 allows [a-zA-Z0-9_.-] only"
            ));
        }
    }
    Ok(())
}

/// Render a stream-name template for Kinesis: [`render_subject`] then
/// [`validate_kinesis_stream`].
///
/// Both checks are load-bearing, exactly as on the Kafka path. `render_subject`
/// refuses an illegal character *inside an interpolated value*, which is what stops
/// a crafted tenant key injecting a `.` separator — still a live risk here because
/// `.` is a legal Kinesis stream character and would pass the charset check.
/// [`validate_kinesis_stream`] then judges the whole rendered string.
///
/// # Errors
///
/// Returns a description of the violation; the caller reports it as
/// [`PublishOutcome::Permanent`].
pub fn render_kinesis_stream(template: &str, ev: &ChangeEvent) -> Result<String, String> {
    let stream = render_subject(template, ev)?;
    validate_kinesis_stream(&stream)?;
    Ok(stream)
}

/// Render a topic template for Kafka: [`render_subject`] then
/// [`validate_kafka_topic`].
///
/// Both checks are load-bearing and neither subsumes the other. `render_subject`
/// refuses an illegal character *inside an interpolated value* — which is what
/// stops a crafted tenant key injecting a `.` separator, still a live risk here
/// because `.` is a legal Kafka topic character and would sail through the
/// charset check. [`validate_kafka_topic`] then judges the whole rendered string
/// against Kafka's narrower charset, catching template literals (`/`, `:`, `+`)
/// that NATS accepts.
///
/// # Errors
///
/// Returns a description of the violation; the caller reports it as
/// [`PublishOutcome::Permanent`].
pub fn render_kafka_topic(template: &str, ev: &ChangeEvent) -> Result<String, String> {
    let topic = render_subject(template, ev)?;
    validate_kafka_topic(&topic)?;
    Ok(topic)
}

#[cfg(test)]
mod tests;
