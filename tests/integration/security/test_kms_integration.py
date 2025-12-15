"""Integration tests for KMS providers (require external services)."""

import pytest

from fraiseql.security.kms import (
    AWSKMSConfig,
    AWSKMSProvider,
    VaultConfig,
    VaultKMSProvider,
)

pytestmark = pytest.mark.integration


@pytest.mark.vault
class TestVaultIntegration:
    """Integration tests for HashiCorp Vault KMS provider.

    Uses testcontainers to automatically provision Vault server.
    Tests skip gracefully if Docker is unavailable.
    """

    @pytest.fixture
    def vault_provider(
        self, vault_url: str, vault_token: str, vault_transit_ready: None
    ) -> VaultKMSProvider:
        """Create Vault KMS provider for testing."""
        config = VaultConfig(
            vault_addr=vault_url,
            token=vault_token,
            mount_path="transit",
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


@pytest.mark.aws
class TestAWSKMSIntegration:
    """Integration tests for AWS KMS provider.

    Uses moto to mock AWS KMS service - no real AWS credentials needed.
    Tests skip gracefully if moto is unavailable.
    """

    @pytest.fixture
    def aws_provider(self, aws_region: str) -> AWSKMSProvider:
        """Create AWS KMS provider for testing."""
        config = AWSKMSConfig(region_name=aws_region)
        return AWSKMSProvider(config)

    @pytest.mark.asyncio
    async def test_encrypt_decrypt_roundtrip(self, aws_provider: AWSKMSProvider, kms_key_id: str):
        """Full encryption/decryption with mocked AWS KMS."""
        test_data = b"Hello, World! This is test data for AWS KMS integration."

        # Encrypt
        encrypted = await aws_provider.encrypt(test_data, key_id=kms_key_id)
        assert encrypted is not None
        assert encrypted.ciphertext != test_data
        assert encrypted.key_reference.key_id == kms_key_id

        # Decrypt
        decrypted = await aws_provider.decrypt(encrypted)
        assert decrypted == test_data

    @pytest.mark.asyncio
    async def test_generate_data_key(self, aws_provider: AWSKMSProvider, kms_key_id: str):
        """Generate data key with AWS KMS."""
        # Generate data key
        data_key = await aws_provider.generate_data_key(key_id=kms_key_id)
        assert data_key is not None
        assert data_key.plaintext_key is not None
        assert data_key.encrypted_key is not None
        assert len(data_key.plaintext_key) == 32  # AES-256 key

    @pytest.mark.asyncio
    async def test_context_encryption(self, aws_provider: AWSKMSProvider, kms_key_id: str):
        """Test encryption with additional authenticated data (context)."""
        test_data = b"Data with context"
        context = {"user_id": "12345", "action": "test"}

        # Encrypt with context
        encrypted = await aws_provider.encrypt(test_data, key_id=kms_key_id, context=context)
        assert encrypted is not None

        # Decrypt with same context should work
        decrypted = await aws_provider.decrypt(encrypted)
        assert decrypted == test_data
