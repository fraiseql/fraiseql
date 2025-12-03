"""Tests for BaseKMSProvider ABC."""

from datetime import UTC, datetime

import pytest

from fraiseql.security.kms.domain.base import BaseKMSProvider
from fraiseql.security.kms.domain.exceptions import (
    DecryptionError,
    EncryptionError,
)
from fraiseql.security.kms.domain.models import (
    DataKeyPair,
    EncryptedData,
    KeyPurpose,
    KeyReference,
)


class ConcreteTestProvider(BaseKMSProvider):
    """Concrete implementation for testing."""

    @property
    def provider_name(self) -> str:
        return "test"

    async def _do_encrypt(
        self,
        plaintext: bytes,
        key_id: str,
        context: dict[str, str],
    ) -> tuple[bytes, str]:
        """Return (ciphertext, algorithm)."""
        return b"encrypted:" + plaintext, "test-algo"

    async def _do_decrypt(
        self,
        ciphertext: bytes,
        key_id: str,
        context: dict[str, str],
    ) -> bytes:
        """Return plaintext."""
        return ciphertext.replace(b"encrypted:", b"")

    async def _do_generate_data_key(
        self,
        key_id: str,
        context: dict[str, str],
    ) -> tuple[bytes, bytes]:
        """Return (plaintext_key, encrypted_key)."""
        return b"0" * 32, b"encrypted-key"

    async def _do_rotate_key(self, key_id: str) -> None:
        pass

    async def _do_get_key_info(self, key_id: str) -> dict:
        return {"alias": None, "created_at": datetime.now(UTC)}

    async def _do_get_rotation_policy(self, key_id: str) -> dict:
        return {"enabled": False, "period_days": 0}


class TestBaseKMSProvider:
    """Tests for BaseKMSProvider."""

    @pytest.fixture
    def provider(self):
        return ConcreteTestProvider()

    @pytest.mark.asyncio
    async def test_encrypt_returns_encrypted_data(self, provider):
        """encrypt() should return EncryptedData with metadata."""
        result = await provider.encrypt(b"plaintext", "my-key")

        assert isinstance(result, EncryptedData)
        assert result.ciphertext == b"encrypted:plaintext"
        assert result.key_reference.provider == "test"
        assert result.key_reference.key_id == "my-key"
        assert result.algorithm == "test-algo"

    @pytest.mark.asyncio
    async def test_encrypt_normalizes_context(self, provider):
        """encrypt() should handle None context."""
        result = await provider.encrypt(b"data", "key", context=None)
        assert result.context == {}

    @pytest.mark.asyncio
    async def test_decrypt_returns_plaintext(self, provider):
        """decrypt() should return plaintext bytes."""
        encrypted = EncryptedData(
            ciphertext=b"encrypted:secret",
            key_reference=KeyReference(
                provider="test",
                key_id="my-key",
                key_alias=None,
                purpose=KeyPurpose.ENCRYPT_DECRYPT,
                created_at=datetime.now(UTC),
            ),
            algorithm="test-algo",
            encrypted_at=datetime.now(UTC),
            context={},
        )

        result = await provider.decrypt(encrypted)

        assert result == b"secret"

    @pytest.mark.asyncio
    async def test_generate_data_key_returns_pair(self, provider):
        """generate_data_key() should return DataKeyPair."""
        result = await provider.generate_data_key("master-key")

        assert isinstance(result, DataKeyPair)
        assert len(result.plaintext_key) == 32
        assert result.encrypted_key.ciphertext == b"encrypted-key"

    def test_cannot_instantiate_abc_directly(self):
        """Should not be able to instantiate ABC without implementing abstracts."""
        with pytest.raises(TypeError):
            BaseKMSProvider()


class TestExceptionWrapping:
    """Tests for exception handling in base class."""

    @pytest.mark.asyncio
    async def test_encrypt_wraps_exceptions(self):
        """Exceptions in _do_encrypt should be wrapped in EncryptionError."""

        class FailingProvider(ConcreteTestProvider):
            async def _do_encrypt(self, plaintext, key_id, context):
                raise RuntimeError("Connection failed")

        provider = FailingProvider()

        with pytest.raises(EncryptionError) as exc_info:
            await provider.encrypt(b"data", "key")

        assert "Encryption operation failed" in str(exc_info.value)

    @pytest.mark.asyncio
    async def test_decrypt_wraps_exceptions(self):
        """Exceptions in _do_decrypt should be wrapped in DecryptionError."""

        class FailingProvider(ConcreteTestProvider):
            async def _do_decrypt(self, ciphertext, key_id, context):
                raise RuntimeError("Invalid ciphertext")

        provider = FailingProvider()
        encrypted = EncryptedData(
            ciphertext=b"bad",
            key_reference=KeyReference(
                provider="test",
                key_id="key",
                key_alias=None,
                purpose=KeyPurpose.ENCRYPT_DECRYPT,
                created_at=datetime.now(UTC),
            ),
            algorithm="test",
            encrypted_at=datetime.now(UTC),
            context={},
        )

        with pytest.raises(DecryptionError) as exc_info:
            await provider.decrypt(encrypted)

        assert "Decryption operation failed" in str(exc_info.value)
