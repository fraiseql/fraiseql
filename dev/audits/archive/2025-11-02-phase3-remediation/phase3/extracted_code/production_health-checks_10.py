# Extracted from: docs/production/health-checks.md
# Block number: 10
async def check_s3_bucket() -> CheckResult:
    """Check S3 bucket accessibility."""
    try:
        s3_client = get_s3_client()

        # Test bucket access
        response = s3_client.head_bucket(Bucket="my-bucket")

        # Get bucket metadata
        objects = s3_client.list_objects_v2(Bucket="my-bucket", MaxKeys=1)
        object_count = objects.get("KeyCount", 0)

        return CheckResult(
            name="s3",
            status=HealthStatus.HEALTHY,
            message="S3 bucket accessible",
            metadata={
                "bucket": "my-bucket",
                "region": s3_client.meta.region_name,
                "object_count": object_count,
            },
        )

    except Exception as e:
        return CheckResult(
            name="s3", status=HealthStatus.UNHEALTHY, message=f"S3 bucket check failed: {e}"
        )
