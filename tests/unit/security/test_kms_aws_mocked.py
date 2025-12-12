"""Unit tests for AWS KMS provider using Moto mocks.

These tests use Moto to mock AWS KMS service, allowing testing without AWS credentials.
"""

import os

import pytest
from moto import mock_aws

from fraiseql.security.kms import AWSKMSConfig, AWSKMSProvider

pytestmark = pytest.mark.unit


@pytest.fixture
def aws_credentials():
    """Mock AWS credentials for moto."""
    os.environ["AWS_ACCESS_KEY_ID"] = "testing"
    os.environ["AWS_SECRET_ACCESS_KEY"] = "testing"
    os.environ["AWS_SECURITY_TOKEN"] = "testing"
    os.environ["AWS_SESSION_TOKEN"] = "testing"
    os.environ["AWS_DEFAULT_REGION"] = "us-east-1"


class TestAWSKMSProviderMocked:
    """Unit tests for AWS KMS provider using Moto mocks."""

    def setup_kms_key(self):
        """Helper to set up a test KMS key."""
        import boto3

        client = boto3.client("kms", region_name="us-east-1")
        # Create a test key
        key_response = client.create_key(
            Description="Test key for FraiseQL unit tests",
            KeyUsage="ENCRYPT_DECRYPT",
        )
        key_id = key_response["KeyMetadata"]["KeyId"]

        # Create an alias for easier reference
        client.create_alias(AliasName="alias/test-key", TargetKeyId=key_id)

        return client, key_id

    @pytest.mark.asyncio
    async def test_encrypt_decrypt_roundtrip(self, aws_credentials):
        """Test full encryption/decryption cycle with mocked AWS KMS."""
        with mock_aws():
            _, key_id = self.setup_kms_key()
            config = AWSKMSConfig(region_name="us-east-1")
            aws_provider = AWSKMSProvider(config)

            test_data = b"Hello, World! This is test data for AWS KMS unit test."

            # Encrypt
            encrypted = await aws_provider.encrypt(test_data, key_id=key_id)
            assert encrypted is not None
            assert encrypted.ciphertext != test_data
            assert encrypted.key_reference.key_id == key_id

            # Decrypt
            decrypted = await aws_provider.decrypt(encrypted)
            assert decrypted == test_data

    @pytest.mark.asyncio
    async def test_encrypt_with_alias(self, aws_credentials):
        """Test encryption using key alias instead of key ID."""
        with mock_aws():
            _, _ = self.setup_kms_key()
            config = AWSKMSConfig(region_name="us-east-1")
            aws_provider = AWSKMSProvider(config)

            test_data = b"Test with alias"

            # Encrypt using alias
            encrypted = await aws_provider.encrypt(test_data, key_id="alias/test-key")
            assert encrypted is not None
            assert encrypted.ciphertext != test_data

            # Decrypt should work
            decrypted = await aws_provider.decrypt(encrypted)
            assert decrypted == test_data

    @pytest.mark.asyncio
    async def test_generate_data_key(self, aws_credentials):
        """Test data key generation with mocked AWS KMS."""
        with mock_aws():
            _, key_id = self.setup_kms_key()
            config = AWSKMSConfig(region_name="us-east-1")
            aws_provider = AWSKMSProvider(config)

            # Generate data key
            data_key = await aws_provider.generate_data_key(key_id=key_id)
            assert data_key is not None
            assert data_key.plaintext_key is not None
            assert data_key.encrypted_key is not None
            assert len(data_key.plaintext_key) == 32  # AES-256 key

    @pytest.mark.asyncio
    async def test_context_encryption(self, aws_credentials):
        """Test encryption with additional authenticated data (context)."""
        with mock_aws():
            _, key_id = self.setup_kms_key()
            config = AWSKMSConfig(region_name="us-east-1")
            aws_provider = AWSKMSProvider(config)

            test_data = b"Data with context"
            context = {"user_id": "12345", "action": "test"}

            # Encrypt with context
            encrypted = await aws_provider.encrypt(test_data, key_id=key_id, context=context)
            assert encrypted is not None

            # Decrypt with same context should work
            decrypted = await aws_provider.decrypt(encrypted)
            assert decrypted == test_data

    @pytest.mark.asyncio
    async def test_different_keys_produce_different_ciphertexts(self, aws_credentials):
        """Test that different keys produce different ciphertexts for same plaintext."""
        with mock_aws():
            import boto3

            config = AWSKMSConfig(region_name="us-east-1")
            aws_provider = AWSKMSProvider(config)

            client = boto3.client("kms", region_name="us-east-1")

            # Create two different keys
            key1_response = client.create_key(Description="Key 1")
            key1_id = key1_response["KeyMetadata"]["KeyId"]

            key2_response = client.create_key(Description="Key 2")
            key2_id = key2_response["KeyMetadata"]["KeyId"]

            test_data = b"Same data, different keys"

            # Encrypt with both keys
            encrypted1 = await aws_provider.encrypt(test_data, key_id=key1_id)
            encrypted2 = await aws_provider.encrypt(test_data, key_id=key2_id)

            # Ciphertexts should be different
            assert encrypted1.ciphertext != encrypted2.ciphertext

            # But both should decrypt to same plaintext
            decrypted1 = await aws_provider.decrypt(encrypted1)
            decrypted2 = await aws_provider.decrypt(encrypted2)
            assert decrypted1 == decrypted2 == test_data

    @pytest.mark.asyncio
    async def test_provider_name(self, aws_credentials):
        """Test that provider name is correct."""
        with mock_aws():
            config = AWSKMSConfig(region_name="us-east-1")
            aws_provider = AWSKMSProvider(config)
            assert aws_provider.provider_name == "aws"

    @pytest.mark.asyncio
    async def test_encrypt_empty_data(self, aws_credentials):
        """Test that AWS KMS rejects empty data (expected behavior)."""
        from fraiseql.security.kms import EncryptionError

        with mock_aws():
            _, key_id = self.setup_kms_key()
            config = AWSKMSConfig(region_name="us-east-1")
            aws_provider = AWSKMSProvider(config)

            test_data = b""

            # AWS KMS does NOT allow encrypting empty data
            with pytest.raises(EncryptionError):
                await aws_provider.encrypt(test_data, key_id=key_id)

    @pytest.mark.asyncio
    async def test_encrypt_large_data(self, aws_credentials):
        """Test encryption of larger data (within KMS limits)."""
        with mock_aws():
            _, key_id = self.setup_kms_key()
            config = AWSKMSConfig(region_name="us-east-1")
            aws_provider = AWSKMSProvider(config)

            # AWS KMS has a 4KB limit for direct encryption
            test_data = b"A" * 4000

            encrypted = await aws_provider.encrypt(test_data, key_id=key_id)
            assert encrypted is not None

            decrypted = await aws_provider.decrypt(encrypted)
            assert decrypted == test_data
