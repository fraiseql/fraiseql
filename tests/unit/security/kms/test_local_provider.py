"""Tests for local development KMS provider."""

import warnings

import pytest

from fraiseql.security.kms.domain.base import BaseKMSProvider
from fraiseql.security.kms.infrastructure.local import (
    LocalKMSConfig,
    LocalKMSProvider,
)


class TestLocalKMSProvider:
    @pytest.fixture
    def provider(self):
        with warnings.catch_warnings():
            warnings.simplefilter("ignore")
            config = LocalKMSConfig(master_key=b"0" * 32)
            return LocalKMSProvider(config)

    def test_extends_base_provider(self, provider):
        assert isinstance(provider, BaseKMSProvider)

    def test_provider_name(self, provider):
        assert provider.provider_name == "local"

    @pytest.mark.asyncio
    async def test_encrypt_decrypt_roundtrip(self, provider):
        """Should encrypt and decrypt data correctly."""
        plaintext = b"sensitive data"

        encrypted = await provider.encrypt(plaintext, "test-key")
        decrypted = await provider.decrypt(encrypted)

        assert decrypted == plaintext

    @pytest.mark.asyncio
    async def test_generate_data_key(self, provider):
        """Should generate a valid data key pair."""
        pair = await provider.generate_data_key("test-key")

        assert len(pair.plaintext_key) == 32
        assert len(pair.encrypted_key.ciphertext) > 0

    def test_warns_about_production_use(self):
        """Should warn that this is for development only."""
        with warnings.catch_warnings(record=True) as w:
            warnings.simplefilter("always")
            LocalKMSProvider()
            assert any("development" in str(warning.message).lower() for warning in w)
