"""AWS KMS integration test fixtures using moto.

Provides mocked AWS KMS service for integration tests without real AWS credentials.
Tests automatically skip if moto is unavailable.
"""

from collections.abc import Generator

import pytest

try:
    import boto3
    from moto import mock_aws

    HAS_MOTO = True
except ImportError:
    HAS_MOTO = False
    mock_aws = None  # type: ignore[assignment, misc]
    boto3 = None  # type: ignore[assignment, misc]


@pytest.fixture
def aws_kms_mock() -> Generator[None]:
    """Start mocked AWS KMS service.

    Scope: function - mock is active per test function
    Skips: Automatically if moto not installed

    Yields:
        None - moto context is active during yield
    """
    if not HAS_MOTO:
        pytest.skip("Moto not available (install with: pip install moto[kms])")

    with mock_aws():
        yield


@pytest.fixture
def aws_kms_client(aws_kms_mock):
    """Get boto3 KMS client configured for mocked AWS.

    Returns:
        boto3 KMS client configured for mocked service
    """
    if not HAS_MOTO or not boto3:
        pytest.skip("Moto/boto3 not available")

    return boto3.client(
        "kms",
        region_name="us-east-1",
        aws_access_key_id="testing",
        aws_secret_access_key="testing",
    )


@pytest.fixture
def kms_key_id(aws_kms_client) -> Generator[str]:
    """Create a test KMS key.

    Scope: function - new key created for each test
    Returns:
        KMS key ID for testing
    """
    if not aws_kms_client:
        pytest.skip("AWS KMS client not available")

    # Create key
    response = aws_kms_client.create_key(
        Description="Test key for FraiseQL integration tests",
        KeyUsage="ENCRYPT_DECRYPT",
    )
    key_id = response["KeyMetadata"]["KeyId"]

    yield key_id

    # Cleanup: schedule key deletion (moto doesn't enforce waiting period)
    try:
        aws_kms_client.schedule_key_deletion(KeyId=key_id, PendingWindowInDays=7)
    except Exception:
        pass  # Best effort cleanup


@pytest.fixture(scope="session")
def aws_region() -> str:
    """Get AWS region for testing.

    Returns:
        AWS region name
    """
    return "us-east-1"
