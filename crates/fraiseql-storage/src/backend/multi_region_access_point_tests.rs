//! A `bucket` naming an S3 Multi-Region Access Point is refused when the backend is
//! created. Such a request can only be signed with `SigV4a`, which this crate's S3 client
//! is built without, so the backend could never serve it.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

use super::create_backend;
use crate::config::StorageConfig;

const MRAP_ARN: &str = "arn:aws:s3::123456789012:accesspoint/mfzwi23gnjvgw.mrap";

fn s3_with(bucket: &str) -> StorageConfig {
    StorageConfig {
        backend:      "s3".to_owned(),
        path:         None,
        bucket:       Some(bucket.to_owned()),
        region:       Some("us-east-1".to_owned()),
        endpoint:     None,
        project_id:   None,
        account_name: None,
    }
}

#[tokio::test]
async fn a_multi_region_access_point_bucket_is_refused_naming_the_key() {
    let Err(error) = create_backend(&s3_with(MRAP_ARN)).await else {
        panic!("a backend was created over a Multi-Region Access Point ARN");
    };
    let message = error.to_string();
    assert!(message.contains("'bucket'"), "{message}");
    assert!(message.contains("Multi-Region Access Point"), "{message}");
    assert!(message.contains("SigV4a"), "{message}");
}

#[cfg(feature = "aws-s3")]
#[tokio::test]
async fn the_refusal_holds_for_every_s3_compatible_provider_and_partition() {
    for backend in [
        "s3",
        "hetzner",
        "scaleway",
        "ovh",
        "exoscale",
        "backblaze",
        "r2",
    ] {
        for arn in [
            MRAP_ARN,
            "arn:aws-cn:s3::123456789012:accesspoint/mfzwi23gnjvgw.mrap",
        ] {
            let config = StorageConfig {
                backend: backend.to_owned(),
                endpoint: Some("https://objects.example.test".to_owned()),
                ..s3_with(arn)
            };
            assert!(create_backend(&config).await.is_err(), "{backend} accepted {arn}");
        }
    }
}

#[cfg(feature = "aws-s3")]
#[tokio::test]
async fn a_plain_bucket_is_accepted() {
    create_backend(&s3_with("my-app-uploads")).await.unwrap();
}

#[cfg(feature = "aws-s3")]
#[tokio::test]
async fn a_regional_access_point_arn_is_accepted() {
    create_backend(&s3_with("arn:aws:s3:us-east-1:123456789012:accesspoint/reports"))
        .await
        .unwrap();
}
