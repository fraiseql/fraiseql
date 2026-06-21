//! The NATS `JetStream` outbound sink (feature `cdc-nats-jetstream`).
//!
//! Publishes each change event to a rendered subject with a `Nats-Msg-Id` header
//! of `{object_type}:{seq}`, which doubles as the consumer dedup key *and*
//! engages `JetStream`'s server-side dedup window. A pure-Rust client
//! (`async-nats`), so this sink adds no C toolchain to the build.

use async_nats::jetstream;

use crate::{
    error::{CdcError, Result},
    event::ChangeEvent,
    sink::{CdcSink, CdcSinkConfig, PublishOutcome, SinkKind, render_subject},
};

/// A sink that publishes change events to NATS `JetStream`.
pub struct NatsJetStreamSink {
    config:    CdcSinkConfig,
    jetstream: jetstream::Context,
}

impl NatsJetStreamSink {
    /// Connect to NATS and build a `JetStream` context for this sink.
    ///
    /// Plaintext `nats://` to a loopback host is refused unless
    /// `FRAISEQL_NATS_ALLOW_PLAINTEXT=true` (dev/CI opt-in, mirroring the
    /// observers transport); use `tls://` in production.
    ///
    /// # Errors
    ///
    /// Returns [`CdcError::Config`] for an unsafe endpoint or
    /// [`CdcError::Connection`] if the NATS connection fails.
    pub async fn connect(url: &str, config: CdcSinkConfig) -> Result<Self> {
        guard_nats_url(url)?;
        let client = async_nats::connect(url)
            .await
            .map_err(|e| CdcError::Connection(format!("connect {url}: {e}")))?;
        let jetstream = jetstream::new(client);
        Ok(Self { config, jetstream })
    }

    /// Ensure a `JetStream` stream exists capturing `subjects` (operator/test
    /// convenience; in production the stream is typically provisioned out of band).
    ///
    /// # Errors
    ///
    /// Returns [`CdcError::Connection`] if the stream cannot be created.
    pub async fn ensure_stream(&self, name: &str, subjects: Vec<String>) -> Result<()> {
        if self.jetstream.get_stream(name).await.is_err() {
            self.jetstream
                .create_stream(jetstream::stream::Config {
                    name: name.to_owned(),
                    subjects,
                    ..Default::default()
                })
                .await
                .map_err(|e| CdcError::Connection(format!("create_stream {name}: {e}")))?;
        }
        Ok(())
    }
}

impl CdcSink for NatsJetStreamSink {
    fn name(&self) -> &str {
        &self.config.name
    }

    fn kind(&self) -> SinkKind {
        SinkKind::NatsJetStream
    }

    fn matches(&self, ev: &ChangeEvent) -> bool {
        self.config.matches(ev)
    }

    async fn publish(&self, ev: &ChangeEvent) -> PublishOutcome {
        let subject = match render_subject(&self.config.subject_template, ev) {
            Ok(subject) => subject,
            Err(reason) => return PublishOutcome::Permanent(format!("subject render: {reason}")),
        };
        let payload = match serde_json::to_vec(ev) {
            Ok(payload) => payload,
            Err(error) => return PublishOutcome::Permanent(format!("encode: {error}")),
        };

        let mut headers = async_nats::HeaderMap::new();
        headers.insert("Nats-Msg-Id", format!("{}:{}", ev.object_type, ev.seq).as_str());

        match self.jetstream.publish_with_headers(subject, headers, payload.into()).await {
            Ok(ack) => match ack.await {
                Ok(_) => PublishOutcome::Published,
                Err(error) => PublishOutcome::Transient(format!("ack: {error}")),
            },
            Err(error) => PublishOutcome::Transient(format!("publish: {error}")),
        }
    }
}

/// Refuse plaintext `nats://` to a loopback host unless explicitly opted in.
fn guard_nats_url(url: &str) -> Result<()> {
    if !url.to_ascii_lowercase().starts_with("nats://") {
        return Ok(()); // tls:// (encrypted) endpoints are always allowed
    }
    let host = url["nats://".len()..].split(['/', ':']).next().unwrap_or_default();
    let is_loopback = matches!(host, "localhost" | "127.0.0.1" | "::1" | "[::1]");
    let allowed = std::env::var("FRAISEQL_NATS_ALLOW_PLAINTEXT").is_ok_and(|v| v == "true");
    if is_loopback && !allowed {
        return Err(CdcError::Config(format!(
            "refusing plaintext nats:// to loopback host {host:?}; use tls:// in production \
             or set FRAISEQL_NATS_ALLOW_PLAINTEXT=true for dev/CI"
        )));
    }
    Ok(())
}
