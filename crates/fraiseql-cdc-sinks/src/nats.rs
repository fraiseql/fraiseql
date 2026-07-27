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
    /// Plaintext `nats://` is refused — the payload is the full row after-image of
    /// every mutation. `tls://` is always accepted; a local dev broker needs
    /// `FRAISEQL_NATS_ALLOW_PLAINTEXT=true` together with a declared development
    /// environment, mirroring the observers transport.
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

/// Env var that permits plaintext `nats://` without transport TLS.
///
/// Honoured only when `FRAISEQL_ENV` declares a development environment. Named
/// and parsed identically to `fraiseql_observers::insecure_guard`'s flag so the
/// two sinks cannot disagree about what the operator asked for.
const NATS_ALLOW_PLAINTEXT_ENV: &str = "FRAISEQL_NATS_ALLOW_PLAINTEXT";

/// Refuse any endpoint that would carry change events unencrypted.
///
/// `tls://` is always allowed. Plaintext `nats://` is refused unless the operator
/// has opted in *and* declared a development environment. Every other scheme is
/// refused outright, including the scheme-less form that `async-nats` silently
/// rewrites to `nats://`.
///
/// The payload on this connection is `serde_json::to_vec(ev)` — the full row
/// after-image of every mutation — so "refused by default" is the only defensible
/// posture.
///
/// This guard shipped **inverted**: it refused plaintext only for loopback hosts,
/// the one case that is safe, and accepted every remote plaintext endpoint. It
/// also skipped all non-`nats://` URLs, split the host with `split(['/', ':'])`
/// so `nats://user:pw@host` yielded `"user"`, and compared the host without
/// lower-casing it. It had no unit tests (#816).
fn guard_nats_url(url: &str) -> Result<()> {
    let lower = url.to_ascii_lowercase();

    if lower.starts_with("tls://") {
        return Ok(());
    }

    if !lower.starts_with("nats://") {
        return Err(CdcError::Config(format!(
            "unsupported NATS scheme in {url:?}: use tls:// for an encrypted connection. \
             A URL with no scheme is rewritten to plaintext nats:// by the client, so it \
             is refused here rather than silently downgraded."
        )));
    }

    if !fraiseql_guard::deployment::insecure_bypass_allowed(fraiseql_guard::deployment::env_opt_in(
        NATS_ALLOW_PLAINTEXT_ENV,
    )) {
        return Err(CdcError::Config(format!(
            "refusing plaintext nats:// endpoint {url:?}: change events would cross the \
             wire in the clear. Use tls://, or set {NATS_ALLOW_PLAINTEXT_ENV}=true with \
             FRAISEQL_ENV=development for dev/CI."
        )));
    }

    // Opted in and in development. The opt-in exists to reach a broker on
    // localhost, so loopback is permitted — but it must not double as a licence
    // to reach the instance-metadata service or an internal network. The host is
    // parsed rather than string-split so that userinfo cannot mask it.
    let host = nats_host(&lower);
    if fraiseql_guard::net::is_loopback_host(host) {
        return Ok(());
    }
    if let Some(reason) = fraiseql_guard::net::blocked_host_reason(host) {
        return Err(CdcError::Config(format!("refusing NATS endpoint {url:?}: {reason}")));
    }

    Ok(())
}

/// Extracts the host from a lower-cased `nats://` URL.
///
/// Strips any `user:password@` userinfo, then takes everything up to the port,
/// path or query. `IPv6` literals keep their brackets, which
/// [`fraiseql_guard::net::blocked_host_reason`] handles.
fn nats_host(lower_url: &str) -> &str {
    let after_scheme = &lower_url["nats://".len()..];
    let authority = after_scheme.split(['/', '?', '#']).next().unwrap_or("");
    let host_port = authority.rsplit_once('@').map_or(authority, |(_, host)| host);
    if let Some(end) = host_port.find(']') {
        // IPv6 literal: the port sits after the closing bracket.
        return &host_port[..=end];
    }
    host_port.split(':').next().unwrap_or(host_port)
}

#[cfg(test)]
mod tests;
