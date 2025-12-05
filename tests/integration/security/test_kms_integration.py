"""Integration tests for KMS providers (require external services)."""

import os

import pytest

from fraiseql.security.kms import (
    AWSKMSConfig,
    AWSKMSProvider,
    VaultConfig,
    VaultKMSProvider,
)

pytestmark = pytest.mark.integration


@pytest.mark.skipif(not os.environ.get("VAULT_ADDR"), reason="Vault not configured")
class TestVaultIntegration:
    """Integration tests for HashiCorp Vault KMS provider."""

    @pytest.fixture
    def vault_provider(self) -> VaultKMSProvider:
        """Create Vault KMS provider for testing."""
        config = VaultConfig(
            vault_addr=os.environ["VAULT_ADDR"],
            token=os.environ["VAULT_TOKEN"],
            mount_path=os.environ.get("VAULT_TRANSIT_MOUNT", "transit"),
        )
        return VaultKMSProvider(config)

    @pytest.mark.asyncio
    async def test_encrypt_decrypt_roundtrip(self, vault_provider: VaultKMSProvider):
        """Full encryption/decryption with real Vault."""
        test_data = b"Hello, World! This is test data for Vault KMS integration."
        key_id = "test-integration-key"

        # Encrypt
        encrypted = await vault_provider.encrypt(test_data, key_id=key_id)
        assert encrypted is not None
        assert encrypted.ciphertext != test_data
        assert encrypted.key_reference.key_id == key_id

        # Decrypt
        decrypted = await vault_provider.decrypt(encrypted)
        assert decrypted == test_data

    @pytest.mark.asyncio
    async def test_data_key_generation(self, vault_provider: VaultKMSProvider):
        """Data key generation with real Vault."""
        key_id = "test-data-key"

        # Generate data key
        data_key = await vault_provider.generate_data_key(key_id=key_id)
        assert data_key is not None
        assert data_key.plaintext_key is not None
        assert data_key.encrypted_key is not None
        assert len(data_key.plaintext_key) == 32  # AES-256 key

    @pytest.mark.asyncio
    async def test_different_keys_isolation(self, vault_provider: VaultKMSProvider):
        """Ensure different keys produce different ciphertexts."""
        test_data = b"Same data, different keys"
        key1 = "test-key-1"
        key2 = "test-key-2"

        encrypted1 = await vault_provider.encrypt(test_data, key_id=key1)
        encrypted2 = await vault_provider.encrypt(test_data, key_id=key2)

        # Different keys should produce different ciphertexts
        assert encrypted1.ciphertext != encrypted2.ciphertext
        assert encrypted1.key_reference.key_id != encrypted2.key_reference.key_id

        # But should decrypt to same plaintext
        decrypted1 = await vault_provider.decrypt(encrypted1)
        decrypted2 = await vault_provider.decrypt(encrypted2)
        assert decrypted1 == decrypted2 == test_data


@pytest.mark.skipif(not os.environ.get("AWS_REGION"), reason="AWS not configured - Manual testing only")
class TestAWSKMSIntegration:
    """Integration tests for AWS KMS provider.

    NOTE: These tests require real AWS credentials and are NOT run in CI/CD for security reasons.
    They remain skipped unless explicitly enabled with AWS environment variables.

    To run manually:
        export AWS_REGION=us-east-1
        export AWS_ACCESS_KEY_ID=your_key
        export AWS_SECRET_ACCESS_KEY=your_secret
        pytest tests/integration/security/test_kms_integration.py::TestAWSKMSIntegration -v

    WARNING: These tests use real AWS resources and may incur costs.
    See: /tmp/UNSKIP_TESTS_PLAN.md Category 2 Phase 2 for details.
    """

    @pytest.fixture
    def aws_provider(self) -> AWSKMSProvider:
        """Create AWS KMS provider for testing."""
        config = AWSKMSConfig(region_name=os.environ["AWS_REGION"])
        return AWSKMSProvider(config)

    @pytest.mark.asyncio
    async def test_encrypt_decrypt_roundtrip(self, aws_provider: AWSKMSProvider):
        """Full encryption/decryption with real AWS KMS."""
        test_data = b"Hello, World! This is test data for AWS KMS integration."
        key_id = "alias/aws/s3"  # Use default key

        # Encrypt
        encrypted = await aws_provider.encrypt(test_data, key_id=key_id)
        assert encrypted is not None
        assert encrypted.ciphertext != test_data
        assert encrypted.key_reference.key_id == key_id

        # Decrypt
        decrypted = await aws_provider.decrypt(encrypted)
        assert decrypted == test_data

    @pytest.mark.asyncio
    async def test_generate_data_key(self, aws_provider: AWSKMSProvider):
        """Generate data key with AWS KMS."""
        key_id = "alias/aws/s3"

        # Generate data key
        data_key = await aws_provider.generate_data_key(key_id=key_id)
        assert data_key is not None
        assert data_key.plaintext_key is not None
        assert data_key.encrypted_key is not None
        assert len(data_key.plaintext_key) == 32  # AES-256 key

    @pytest.mark.asyncio
    async def test_context_encryption(self, aws_provider: AWSKMSProvider):
        """Test encryption with additional authenticated data (context)."""
        test_data = b"Data with context"
        context = {"user_id": "12345", "action": "test"}

        # Encrypt with context
        encrypted = await aws_provider.encrypt(test_data, key_id="alias/aws/s3", context=context)
        assert encrypted is not None

        # Decrypt with same context should work
        decrypted = await aws_provider.decrypt(encrypted)
        assert decrypted == test_data
