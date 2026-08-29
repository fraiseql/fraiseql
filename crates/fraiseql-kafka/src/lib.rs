//! One Kafka egress for the whole framework (#1102).
//!
//! # Why one
//!
//! Two paths in FraiseQL produce to Kafka, and they are genuinely different above the
//! wire: the CDC outbox drain is at-least-once with a claim, a retry schedule and a
//! dead-letter state, while a subscription delivery is at-most-once with no outbox at
//! all. Below the wire they must not differ. A second producer means a second answer to
//! "may this endpoint be plaintext?", a second `security.protocol`, a second SASL
//! resolution — and the two answers drift. That is not a hypothetical: #816 shipped an
//! inverted plaintext guard on the NATS transport, and #1102 was filed because the
//! subscription transport set no `security.protocol` at all while the CDC sink had a
//! screened one.
//!
//! So this crate owns exactly the shared part — connect and produce — and nothing else.
//! **Durability stays with the caller.** There is no retry here, no queue, no
//! acknowledgement bookkeeping: [`KafkaEgress::send`] reports what happened and the
//! caller decides what that means.
//!
//! # Where the pieces live
//!
//! | | |
//! |---|---|
//! | Endpoint parsing, plaintext refusal, SASL resolution | [`fraiseql_guard::kafka`] |
//! | Connect, `security.protocol`, produce | here |
//! | Outbox, retry, dead-letter | `fraiseql-cdc-sinks` |
//! | Topic rendering, partition key, payload encoding | each caller |
//!
//! The guard is one crate lower because it names no rdkafka type, which keeps its
//! refusing half in the cheap always-compiled test leg.
//!
//! # Example
//!
//! ```no_run
//! use fraiseql_kafka::{EgressRecord, KafkaEgress, KafkaEgressConfig};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let egress = KafkaEgress::connect(
//!     "kafka+ssl://broker.example.com:9093",
//!     &KafkaEgressConfig::new("fraiseql-subscriptions"),
//! )?;
//!
//! let outcome = egress
//!     .send(EgressRecord {
//!         topic:   "orders",
//!         key:     "order-42",
//!         payload: b"{}",
//!         headers: &[("fraiseql-op", "INSERT")],
//!     })
//!     .await;
//! # let _ = outcome;
//! # Ok(())
//! # }
//! ```

#![forbid(unsafe_code)]

use std::time::Duration;

use fraiseql_guard::kafka::{KafkaSecurityProtocol, guard_kafka_endpoint, resolve_kafka_sasl};
/// The `rdkafka` this crate's public API is built against (#1198).
pub use rdkafka;
use rdkafka::{
    ClientConfig,
    error::KafkaError,
    message::{Header, OwnedHeaders},
    producer::{FutureProducer, FutureRecord},
    types::RDKafkaErrorCode,
};

/// Why an egress could not be built.
///
/// `send` has no error channel — see [`EgressOutcome`] — so this is a connect-time type
/// only. The two variants exist because callers report them differently: a bad endpoint
/// is the operator's to fix, an unreachable broker is not.
#[derive(Debug, thiserror::Error)]
pub enum EgressError {
    /// The endpoint was refused: no scheme, an unsupported one, a malformed broker list,
    /// unopted-in plaintext, a blocked broker host, or unresolvable SASL credentials.
    #[error("{0}")]
    Config(String),
    /// The endpoint was accepted but librdkafka could not create a client for it.
    #[error("{0}")]
    Connection(String),
}

/// Producer settings a caller chooses, because they follow from *its* delivery contract.
///
/// Nothing here is a security setting: `security.protocol` and SASL come from the
/// endpoint's scheme via [`fraiseql_guard::kafka`], and no caller can override them.
#[derive(Debug, Clone)]
pub struct KafkaEgressConfig {
    /// `client.id` — what this producer calls itself in broker logs and quotas.
    pub client_id: String,

    /// `enable.idempotence`. On, librdkafka holds `acks=all`, bounds in-flight requests
    /// and retries producer-side, which together give no-duplicates-from-retry *and*
    /// per-partition ordering. With a key that pins each entity to a partition, that is
    /// per-entity ordering.
    ///
    /// Defaults on. A caller turns it off only to accept duplicates for throughput.
    pub idempotent: bool,

    /// `message.timeout.ms` — librdkafka's own end-to-end delivery attempt.
    ///
    /// ⚠ Sized to the caller's *retry cadence*, not to a general notion of "long
    /// enough". The CDC drain wants 30s, well under its backoff, so a broker outage
    /// produces a classified transient outcome per attempt rather than an unbounded
    /// stall. A subscription delivery has no retry at all, so 30s there would park a
    /// hot-path task for half a minute over a message nobody will resend.
    pub message_timeout: Duration,

    /// How long [`KafkaEgress::send`] may wait for space in the local producer queue.
    ///
    /// Bounded for the same reason: a full queue must surface as an outcome the caller
    /// can act on, not park the task.
    pub enqueue_timeout: Duration,

    /// `compression.type` (`"lz4"`, `"gzip"`, `"snappy"`, `"zstd"`), or `None`.
    pub compression: Option<String>,
}

impl KafkaEgressConfig {
    /// Defaults suited to a durable caller: idempotent, 30s delivery, 5s enqueue, lz4.
    ///
    /// A caller without a retry loop should shorten both timeouts — see
    /// [`message_timeout`](Self::message_timeout).
    #[must_use]
    pub fn new(client_id: impl Into<String>) -> Self {
        Self {
            client_id:       client_id.into(),
            idempotent:      true,
            message_timeout: Duration::from_secs(30),
            enqueue_timeout: Duration::from_secs(5),
            compression:     Some("lz4".to_owned()),
        }
    }

    /// Set `message.timeout.ms` and the enqueue bound together.
    ///
    /// One method rather than two because setting one without the other is how a caller
    /// ends up with a hot path that blocks for the *other* one's budget.
    #[must_use]
    pub const fn with_timeouts(mut self, message: Duration, enqueue: Duration) -> Self {
        self.message_timeout = message;
        self.enqueue_timeout = enqueue;
        self
    }

    /// Turn producer idempotence off.
    #[must_use]
    pub const fn without_idempotence(mut self) -> Self {
        self.idempotent = false;
        self
    }

    /// Set or clear `compression.type`.
    #[must_use]
    pub fn with_compression(mut self, compression: Option<String>) -> Self {
        self.compression = compression;
        self
    }
}

/// One record to produce. Borrowed throughout: the caller owns the bytes it rendered.
///
/// Deliberately not `#[non_exhaustive]`: this is an input a caller builds, and the
/// attribute would make the struct literal — the only ergonomic way to build it —
/// impossible from outside this crate. A new field here is a breaking change on purpose.
#[derive(Debug)]
pub struct EgressRecord<'a> {
    /// The topic, already rendered and validated by the caller — this crate does not
    /// know either caller's naming rules and must not invent one.
    pub topic:   &'a str,
    /// The partition key. Same key ⇒ same partition ⇒ ordered, which is the whole of
    /// per-entity ordering.
    pub key:     &'a str,
    /// The encoded payload.
    pub payload: &'a [u8],
    /// Headers, as `(name, value)` pairs.
    pub headers: &'a [(&'a str, &'a str)],
}

/// What became of one produce attempt.
///
/// There is no error channel, deliberately: every failure is classified, so a caller
/// cannot accidentally treat "the broker is down" and "this message can never be
/// accepted" the same way by writing `?`.
///
/// Not `#[non_exhaustive]`, also deliberately. A caller's whole job is to decide what
/// each outcome means for its own durability, and a `_` arm is where a new outcome gets
/// silently folded into the wrong one. A new variant should be a compile error at every
/// match site — the same reason `DatabaseType` stays exhaustive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EgressOutcome {
    /// The broker acknowledged the record.
    Delivered,
    /// Redelivering the same bytes could succeed — broker down, ack timeout, full queue.
    Transient(String),
    /// Redelivering the same bytes cannot succeed — too large, invalid topic or record.
    Permanent(String),
}

/// A connected Kafka producer, and the only one this framework builds.
pub struct KafkaEgress {
    producer:        FutureProducer,
    enqueue_timeout: Duration,
}

impl std::fmt::Debug for KafkaEgress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KafkaEgress")
            .field("enqueue_timeout", &self.enqueue_timeout)
            .finish_non_exhaustive()
    }
}

impl KafkaEgress {
    /// Guard the endpoint, then build a producer for it.
    ///
    /// `endpoint` must carry an explicit scheme — `kafka+ssl://`, `kafka+sasl-ssl://`,
    /// or plaintext `kafka://` under the development opt-in. A scheme-less endpoint is
    /// **refused rather than defaulted**: librdkafka reads a bare `bootstrap.servers`
    /// list as `PLAINTEXT`, which is the silent downgrade #816 shipped.
    ///
    /// `security.protocol` is always set from the scheme, never left to the client's
    /// default, and no caller can override it.
    ///
    /// # Errors
    ///
    /// [`EgressError::Config`] for a refused endpoint or unresolvable SASL credentials;
    /// [`EgressError::Connection`] if librdkafka cannot create the client.
    pub fn connect(endpoint: &str, config: &KafkaEgressConfig) -> Result<Self, EgressError> {
        let guarded = guard_kafka_endpoint(endpoint).map_err(EgressError::Config)?;

        let mut client = ClientConfig::new();
        client
            .set("bootstrap.servers", &guarded.bootstrap_servers)
            .set("security.protocol", guarded.security_protocol.as_str())
            .set("client.id", &config.client_id)
            .set("enable.idempotence", if config.idempotent { "true" } else { "false" })
            .set("message.timeout.ms", config.message_timeout.as_millis().to_string());

        if let Some(ref compression) = config.compression {
            client.set("compression.type", compression);
        }

        if guarded.security_protocol == KafkaSecurityProtocol::SaslSsl {
            let sasl = resolve_kafka_sasl().map_err(EgressError::Config)?;
            client
                .set("sasl.mechanism", sasl.mechanism.as_str())
                .set("sasl.username", &sasl.username)
                .set("sasl.password", &sasl.password);
        }

        let producer: FutureProducer = client
            .create()
            .map_err(|e| EgressError::Connection(format!("kafka producer: {e}")))?;

        tracing::info!(
            security_protocol = guarded.security_protocol.as_str(),
            client_id = %config.client_id,
            idempotent = config.idempotent,
            "kafka egress connected"
        );

        Ok(Self {
            producer,
            enqueue_timeout: config.enqueue_timeout,
        })
    }

    /// Whether the broker answers a metadata request within `timeout`.
    ///
    /// Here rather than in a caller because it needs the producer's client, and the
    /// producer is this crate's to own — a caller reaching for the raw client to answer
    /// "is the broker up?" is the first step back towards a second producer.
    ///
    /// A `false` is not a delivery verdict: librdkafka fetches metadata lazily on a
    /// background thread, so a producer for an unreachable broker still exists and a
    /// send against it fails later, transiently.
    #[must_use]
    pub fn is_reachable(&self, timeout: Duration) -> bool {
        use rdkafka::producer::Producer;

        match self.producer.client().fetch_metadata(None, timeout) {
            Ok(metadata) => {
                tracing::debug!(
                    brokers = metadata.brokers().len(),
                    topics = metadata.topics().len(),
                    "kafka egress reachable"
                );
                true
            },
            Err(error) => {
                tracing::warn!(%error, "kafka egress unreachable");
                false
            },
        }
    }

    /// Produce one record and classify the result.
    ///
    /// Never retries and never blocks past the configured enqueue timeout. What a
    /// [`EgressOutcome::Transient`] means is the caller's to decide: the CDC drain
    /// schedules another attempt against its own ceiling, and the subscription
    /// transport, being at-most-once, logs and drops.
    pub async fn send(&self, record: EgressRecord<'_>) -> EgressOutcome {
        let mut headers = OwnedHeaders::new();
        for &(key, value) in record.headers {
            headers = headers.insert(Header {
                key,
                value: Some(value),
            });
        }

        let future_record = FutureRecord::to(record.topic)
            .key(record.key)
            .payload(record.payload)
            .headers(headers);

        match self.producer.send(future_record, self.enqueue_timeout).await {
            Ok(_) => EgressOutcome::Delivered,
            Err((error, _)) => classify(&error),
        }
    }
}

/// Classify a produce failure.
///
/// The default is [`EgressOutcome::Transient`], deliberately. A caller with a retry
/// ceiling loses a delay on a misclassified transient error and loses the *message* on a
/// misclassified permanent one, so only failures that cannot succeed on redelivery of
/// the same bytes are permanent.
///
/// ⚠ For a caller with no ceiling — an at-most-once transport — this classification is
/// advisory rather than an instruction: `Transient` says "these bytes could have gone
/// through later", which is a log level, not a retry.
fn classify(error: &KafkaError) -> EgressOutcome {
    let permanent = matches!(
        error,
        KafkaError::MessageProduction(
            RDKafkaErrorCode::MessageSizeTooLarge
                | RDKafkaErrorCode::MessageBatchTooLarge
                | RDKafkaErrorCode::InvalidTopic
                | RDKafkaErrorCode::InvalidRecord
        )
    );
    if permanent {
        EgressOutcome::Permanent(format!("publish: {error}"))
    } else {
        EgressOutcome::Transient(format!("publish: {error}"))
    }
}

#[cfg(test)]
mod tests;
