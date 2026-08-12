//! The AWS Kinesis Data Streams outbound sink (feature `cdc-kinesis`).
//!
//! Publishes each change event to a rendered stream with `PutRecord`. The region
//! parsing, endpoint-override guard, stream-name validation and partition key this
//! sink depends on all live in [`crate::sink`] and are always compiled — see the
//! note there. This module holds only what genuinely needs the AWS SDK.

use aws_sdk_kinesis::{
    Client, error::SdkError, operation::put_record::PutRecordError, primitives::Blob,
};

use crate::{
    error::Result,
    event::ChangeEvent,
    sink::{
        CdcSink, CdcSinkConfig, PublishOutcome, SinkKind, entity_partition_key,
        guard_kinesis_endpoint, render_kinesis_stream, resolve_kinesis_endpoint_url,
    },
};

/// A sink that publishes change events to AWS Kinesis Data Streams.
pub struct KinesisSink {
    config: CdcSinkConfig,
    client: Client,
}

impl KinesisSink {
    /// Build a Kinesis client for this sink.
    ///
    /// `endpoint` names the region as `kinesis://<region>` and is screened by
    /// [`guard_kinesis_endpoint`] before any client is constructed. Credentials
    /// come from the standard AWS provider chain (environment, profile, IMDS, IRSA
    /// for EKS) rather than from sink config, because they are secrets and the
    /// config is destined for a TOML surface.
    ///
    /// Transport is HTTPS: the SDK resolves the regional endpoint itself. The one
    /// way to reach a plaintext endpoint is `FRAISEQL_KINESIS_ENDPOINT_URL`, which
    /// [`resolve_kinesis_endpoint_url`] refuses unless the operator has opted in,
    /// declared a development environment, and pointed it at loopback.
    ///
    /// # Errors
    ///
    /// Returns [`CdcError::Config`](crate::error::CdcError::Config) for an
    /// unsafe or malformed endpoint or
    /// endpoint-URL override.
    pub async fn connect(endpoint: &str, config: CdcSinkConfig) -> Result<Self> {
        let endpoint = guard_kinesis_endpoint(endpoint)?;
        let override_url = resolve_kinesis_endpoint_url()?;

        // `BehaviorVersion::latest()` is set explicitly: the SDK panics at client
        // construction if no behaviour version is pinned, and the alternative —
        // the `behavior-version-latest` cargo feature — hides that choice in a
        // manifest where it reads as incidental.
        let mut loader = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(aws_config::Region::new(endpoint.region.clone()));
        if let Some(url) = override_url {
            loader = loader.endpoint_url(url);
        }
        let aws = loader.load().await;

        Ok(Self {
            config,
            client: Client::new(&aws),
        })
    }
}

impl CdcSink for KinesisSink {
    fn name(&self) -> &str {
        &self.config.name
    }

    fn kind(&self) -> SinkKind {
        SinkKind::Kinesis
    }

    fn matches(&self, ev: &ChangeEvent) -> bool {
        self.config.matches(ev)
    }

    async fn publish(&self, ev: &ChangeEvent) -> PublishOutcome {
        let stream = match render_kinesis_stream(&self.config.subject_template, ev) {
            Ok(stream) => stream,
            Err(reason) => return PublishOutcome::Permanent(format!("stream render: {reason}")),
        };
        let payload = match serde_json::to_vec(ev) {
            Ok(payload) => payload,
            Err(error) => return PublishOutcome::Permanent(format!("encode: {error}")),
        };

        // No `sequence_number_for_ordering`: it would require holding the previous
        // record's sequence number per partition key, which is unbounded state for
        // a firehose, and it buys nothing here. The drain publishes strictly
        // serially and head-of-line-blocks an unfinished row, so record N is
        // durably stored before N+1 is sent — arrival order already is `seq` order.
        // Consumers dedup on the `(object_type, seq)` pair in the payload.
        let result = self
            .client
            .put_record()
            .stream_name(&stream)
            .partition_key(entity_partition_key(ev))
            .data(Blob::new(payload))
            .send()
            .await;

        match result {
            Ok(_) => PublishOutcome::Published,
            Err(error) => classify(&error),
        }
    }
}

/// Classify a `PutRecord` failure as retryable or dead-letter.
///
/// The default is [`PublishOutcome::Transient`], deliberately, for the reason the
/// Kafka sink states: the drain dead-letters on its own `max_attempts` ceiling
/// anyway, so a misclassified transient error costs a delay, while a misclassified
/// permanent one discards a change event. Only failures that *cannot* succeed on
/// redelivery of the same bytes are permanent.
///
/// `InvalidArgumentException` is the only modelled `PutRecord` failure that
/// qualifies — a payload over the record limit, or a partition key Kinesis rejects,
/// fails identically on every redelivery of the same bytes.
///
/// Everything else retries. `ResourceNotFoundException` in particular is
/// **transient**, not permanent: a stream that does not exist yet may be created,
/// and dead-lettering the backlog for a stream an operator is mid-way through
/// provisioning would lose events no retry needed to lose. Throttling
/// (`ProvisionedThroughputExceededException`) is the ordinary Kinesis back-pressure
/// signal and is exactly what backoff is for, and `AccessDeniedException` is
/// commonly an IAM propagation delay.
///
/// The message is built from the modelled error's `Display`, never the `SdkError`'s
/// `Debug`: the latter can carry the raw HTTP response, and this string is
/// persisted to the `last_error` column.
fn classify<R>(error: &SdkError<PutRecordError, R>) -> PublishOutcome {
    match error {
        SdkError::ServiceError(service) => classify_service(service.err()),
        SdkError::TimeoutError(_) => PublishOutcome::Transient("publish: timeout".to_owned()),
        SdkError::DispatchFailure(_) => {
            PublishOutcome::Transient("publish: dispatch failure".to_owned())
        },
        SdkError::ResponseError(_) => {
            PublishOutcome::Transient("publish: malformed response".to_owned())
        },
        SdkError::ConstructionFailure(_) => {
            PublishOutcome::Transient("publish: request construction failed".to_owned())
        },
        _ => PublishOutcome::Transient("publish: unknown SDK failure".to_owned()),
    }
}

/// The service-error half of [`classify`] — the part that carries the actual
/// transient-vs-permanent decision.
///
/// Split out so it is testable directly: constructing an [`SdkError::ServiceError`]
/// requires a raw HTTP response, whose types are not re-exported by
/// `aws-sdk-kinesis`, and the classification rule should not go untested for want
/// of two dev-dependencies.
fn classify_service(error: &PutRecordError) -> PublishOutcome {
    let detail = format!("publish: {error}");
    if matches!(error, PutRecordError::InvalidArgumentException(_)) {
        PublishOutcome::Permanent(detail)
    } else {
        PublishOutcome::Transient(detail)
    }
}

#[cfg(test)]
mod tests;
