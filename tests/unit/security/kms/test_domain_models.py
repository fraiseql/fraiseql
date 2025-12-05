"""Tests for KMS domain models."""

from datetime import UTC, datetime

import pytest

from fraiseql.security.kms.domain.models import (
    DataKeyPair,
    EncryptedData,
    KeyPurpose,
    KeyReference,
    RotationPolicy,
)


class TestKeyReference:
    """Tests for KeyReference value object."""

    def test_is_immutable(self):
        """KeyReference should be immutable (frozen dataclass)."""
        ref = KeyReference(
            provider="vault",
            key_id="my-key",
            key_alias="alias/my-key",
            purpose=KeyPurpose.ENCRYPT_DECRYPT,
            created_at=datetime.now(UTC),
        )
        with pytest.raises((AttributeError, TypeError)):
            ref.key_id = "other-key"

    def test_qualified_id(self):
        """Should generate qualified ID as provider:key_id."""
        ref = KeyReference(
            provider="vault",
            key_id="my-key",
            key_alias=None,
            purpose=KeyPurpose.ENCRYPT_DECRYPT,
            created_at=datetime.now(UTC),
        )
        assert ref.qualified_id == "vault:my-key"

    def test_equality(self):
        """Two references with same values should be equal."""
        now = datetime.now(UTC)
        ref1 = KeyReference(
            provider="vault",
            key_id="my-key",
            key_alias=None,
            purpose=KeyPurpose.ENCRYPT_DECRYPT,
            created_at=now,
        )
        ref2 = KeyReference(
            provider="vault",
            key_id="my-key",
            key_alias=None,
            purpose=KeyPurpose.ENCRYPT_DECRYPT,
            created_at=now,
        )
        assert ref1 == ref2


class TestEncryptedData:
    """Tests for EncryptedData value object."""

    def test_to_dict_serialization(self):
        """Should serialize to dictionary correctly."""
        now = datetime.now(UTC)
        key_ref = KeyReference(
            provider="vault",
            key_id="my-key",
            key_alias=None,
            purpose=KeyPurpose.ENCRYPT_DECRYPT,
            created_at=now,
        )
        encrypted = EncryptedData(
            ciphertext=b"encrypted-data",
            key_reference=key_ref,
            algorithm="aes256-gcm96",
            encrypted_at=now,
            context={"purpose": "test"},
        )

        result = encrypted.to_dict()

        assert result["ciphertext"] == "656e637279707465642d64617461"  # hex
        assert result["key_id"] == "vault:my-key"
        assert result["algorithm"] == "aes256-gcm96"
        assert result["context"] == {"purpose": "test"}


class TestDataKeyPair:
    """Tests for DataKeyPair value object."""

    def test_contains_both_keys(self):
        """Should contain both plaintext and encrypted keys."""
        now = datetime.now(UTC)
        key_ref = KeyReference(
            provider="vault",
            key_id="master-key",
            key_alias=None,
            purpose=KeyPurpose.ENCRYPT_DECRYPT,
            created_at=now,
        )
        encrypted_key = EncryptedData(
            ciphertext=b"encrypted-key",
            key_reference=key_ref,
            algorithm="aes256-gcm96",
            encrypted_at=now,
            context={},
        )

        pair = DataKeyPair(
            plaintext_key=b"12345678901234567890123456789012",  # 32 bytes exactly
            encrypted_key=encrypted_key,
            key_reference=key_ref,
        )

        assert len(pair.plaintext_key) == 32
        assert pair.encrypted_key.ciphertext == b"encrypted-key"


class TestKeyPurpose:
    """Tests for KeyPurpose enum."""

    def test_encrypt_decrypt_value(self):
        assert KeyPurpose.ENCRYPT_DECRYPT.value == "encrypt_decrypt"

    def test_sign_verify_value(self):
        assert KeyPurpose.SIGN_VERIFY.value == "sign_verify"


class TestRotationPolicy:
    """Tests for RotationPolicy value object."""

    def test_disabled_rotation(self):
        policy = RotationPolicy(
            enabled=False,
            rotation_period_days=0,
            last_rotation=None,
            next_rotation=None,
        )
        assert policy.enabled is False

    def test_enabled_rotation_with_schedule(self):
        now = datetime.now(UTC)
        policy = RotationPolicy(
            enabled=True,
            rotation_period_days=90,
            last_rotation=now,
            next_rotation=None,
        )
        assert policy.enabled is True
        assert policy.rotation_period_days == 90
