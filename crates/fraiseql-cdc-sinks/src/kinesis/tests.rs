//! Tests for the parts of the Kinesis sink that need the AWS SDK in scope.
//!
//! The endpoint guard, the endpoint-URL override guard and the stream charset are
//! pure and tested in `crate::sink::tests`, where they run in the always-compiled
//! unit-test leg. What is left here is that the sink actually *consults* them, plus
//! the `PutRecord` error classification — which names SDK types.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::KinesisSink;
use crate::sink::{CdcSink, CdcSinkConfig, SinkKind};

/// Dummy static credentials, so `load()` resolves from the environment instead of
/// walking the provider chain out to IMDS (which would make these tests wait on a
/// network timeout in CI).
fn creds_plus<'a>(vars: Vec<(&'a str, Option<&'a str>)>) -> Vec<(&'a str, Option<&'a str>)> {
    let mut all = vec![
        ("AWS_ACCESS_KEY_ID", Some("test")),
        ("AWS_SECRET_ACCESS_KEY", Some("test")),
        ("AWS_EC2_METADATA_DISABLED", Some("true")),
    ];
    all.extend(vars);
    all
}

#[tokio::test]
async fn connect_consults_the_endpoint_guard() {
    // Belt-and-braces over the pure `guard_kinesis_endpoint` tests: assert the sink
    // actually calls it, so a refactor cannot leave a scheme-less region accepted.
    // `Box::pin`: the AWS SDK's client-construction future is large enough to trip
    // `clippy::large_futures` on the stack.
    let refused = Box::pin(temp_env::async_with_vars(
        creds_plus(vec![("FRAISEQL_KINESIS_ENDPOINT_URL", None)]),
        async {
            let cfg = CdcSinkConfig::new("primary", "fraiseql.{table}");
            KinesisSink::connect("us-east-1", cfg).await
        },
    ))
    .await;
    let err = refused.err().expect("a scheme-less endpoint must be refused").to_string();
    assert!(err.contains("kinesis://"), "should be our named refusal: {err}");
}

#[tokio::test]
async fn connect_consults_the_endpoint_url_guard() {
    // The plaintext override must be refused by the sink, not merely by the pure
    // helper. Loopback host, so the refusal is attributable to the missing opt-in.
    let refused = Box::pin(temp_env::async_with_vars(
        creds_plus(vec![
            ("FRAISEQL_KINESIS_ENDPOINT_URL", Some("http://localhost:4566")),
            ("FRAISEQL_KINESIS_ALLOW_PLAINTEXT", None),
        ]),
        async {
            let cfg = CdcSinkConfig::new("primary", "fraiseql.{table}");
            KinesisSink::connect("kinesis://us-east-1", cfg).await
        },
    ))
    .await;
    let err = refused
        .err()
        .expect("an unopted-in http:// override must be refused")
        .to_string();
    assert!(
        err.contains("FRAISEQL_KINESIS_ALLOW_PLAINTEXT"),
        "should name the opt-in: {err}"
    );
}

#[tokio::test]
async fn connect_builds_a_client_for_a_valid_region() {
    // The accepting half, assertable without a broker: no request is sent at
    // construction, so an unreachable endpoint still yields a client.
    let sink = Box::pin(temp_env::async_with_vars(
        creds_plus(vec![("FRAISEQL_KINESIS_ENDPOINT_URL", None)]),
        async {
            let cfg = CdcSinkConfig::new("primary", "fraiseql.{table}");
            KinesisSink::connect("kinesis://eu-west-3", cfg).await
        },
    ))
    .await;
    let sink = sink.expect("a valid kinesis:// region must be constructible");
    assert_eq!(sink.kind(), SinkKind::Kinesis);
    assert_eq!(sink.name(), "primary");
}

#[tokio::test]
async fn connect_accepts_a_loopback_plaintext_override_when_opted_in() {
    // The LocalStack path the e2e suite depends on.
    let sink = Box::pin(temp_env::async_with_vars(
        creds_plus(vec![
            ("FRAISEQL_KINESIS_ENDPOINT_URL", Some("http://127.0.0.1:4566")),
            ("FRAISEQL_KINESIS_ALLOW_PLAINTEXT", Some("true")),
            ("FRAISEQL_ENV", Some("development")),
            ("FRAISEQL_PROFILE", None),
            ("KUBERNETES_SERVICE_HOST", None),
        ]),
        async {
            let cfg = CdcSinkConfig::new("primary", "fraiseql.{table}");
            KinesisSink::connect("kinesis://us-east-1", cfg).await
        },
    ))
    .await;
    assert!(sink.is_ok(), "{:?}", sink.err().map(|e| e.to_string()));
}

#[test]
fn only_invalid_argument_is_permanent_and_everything_else_retries() {
    use aws_sdk_kinesis::{
        operation::put_record::PutRecordError,
        types::error::{
            AccessDeniedException, InternalFailureException, InvalidArgumentException,
            ProvisionedThroughputExceededException, ResourceNotFoundException,
        },
    };

    use super::classify_service;
    use crate::sink::PublishOutcome;

    // A rejected argument — an oversized record, a partition key Kinesis will not
    // take — fails identically on every redelivery of the same bytes.
    assert!(matches!(
        classify_service(&PutRecordError::InvalidArgumentException(
            InvalidArgumentException::builder().message("bad partition key").build()
        )),
        PublishOutcome::Permanent(_)
    ));

    // Everything else must retry rather than discard the change event: a missing
    // stream may still be created, throttling is what backoff is for, an IAM denial
    // is commonly a propagation delay, and an internal failure is AWS's problem.
    assert!(matches!(
        classify_service(&PutRecordError::ResourceNotFoundException(
            ResourceNotFoundException::builder().message("no such stream").build()
        )),
        PublishOutcome::Transient(_)
    ));
    assert!(matches!(
        classify_service(&PutRecordError::ProvisionedThroughputExceededException(
            ProvisionedThroughputExceededException::builder().message("slow down").build()
        )),
        PublishOutcome::Transient(_)
    ));
    assert!(matches!(
        classify_service(&PutRecordError::AccessDeniedException(
            AccessDeniedException::builder().message("denied").build()
        )),
        PublishOutcome::Transient(_)
    ));
    assert!(matches!(
        classify_service(&PutRecordError::InternalFailureException(
            InternalFailureException::builder().message("boom").build()
        )),
        PublishOutcome::Transient(_)
    ));
}
