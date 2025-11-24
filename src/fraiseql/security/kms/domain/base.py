"""Base KMS provider abstract class.

Provides template methods with common logic and abstract hooks
for provider-specific implementations.
"""

import logging
from abc import ABC, abstractmethod
from datetime import UTC, datetime

from fraiseql.security.kms.domain.exceptions import (
    DecryptionError,
    EncryptionError,
    KeyRotationError,
)
from fraiseql.security.kms.domain.models import (
    DataKeyPair,
    EncryptedData,
    KeyPurpose,
    KeyReference,
    RotationPolicy,
)

logger = logging.getLogger(__name__)


class BaseKMSProvider(ABC):
    """Abstract base class for KMS providers.

    Implements the Template Method pattern:
    - Public methods (encrypt, decrypt, etc.) handle common logic
    - Protected abstract methods (_do_encrypt, _do_decrypt, etc.) are
      implemented by concrete providers

    Subclasses must implement:
    - provider_name (property)
    - _do_encrypt()
    - _do_decrypt()
    - _do_generate_data_key()
    - _do_rotate_key()
    - _do_get_key_info()
    - _do_get_rotation_policy()
    """

    @property
    @abstractmethod
    def provider_name(self) -> str:
        """Unique provider identifier (e.g., 'vault', 'aws', 'gcp')."""
        ...

    # ─────────────────────────────────────────────────────────────
    # Template Methods (public API)
    # ─────────────────────────────────────────────────────────────

    async def encrypt(
        self,
        plaintext: bytes,
        key_id: str,
        *,
        context: dict[str, str] | None = None,
    ) -> EncryptedData:
        """Encrypt data using the specified key.

        Args:
            plaintext: Data to encrypt
            key_id: Key identifier
            context: Additional authenticated data (AAD)

        Returns:
            EncryptedData with ciphertext and metadata

        Raises:
            EncryptionError: If encryption fails
        """
        ctx = context or {}
        logger.debug(
            "Encrypting %d bytes with key %s",
            len(plaintext),
            key_id,
        )

        try:
            ciphertext, algorithm = await self._do_encrypt(plaintext, key_id, ctx)

            return EncryptedData(
                ciphertext=ciphertext,
                key_reference=KeyReference(
                    provider=self.provider_name,
                    key_id=key_id,
                    key_alias=None,
                    purpose=KeyPurpose.ENCRYPT_DECRYPT,
                    created_at=datetime.now(UTC),
                ),
                algorithm=algorithm,
                encrypted_at=datetime.now(UTC),
                context=ctx,
            )
        except EncryptionError:
            raise
        except Exception as e:
            # SECURITY: Log full error for debugging, but sanitize message to caller
            # to avoid leaking sensitive info (key IDs, vault paths, AWS ARNs)
            logger.exception("Encryption failed for key %s", key_id)
            raise EncryptionError("Encryption operation failed") from e

    async def decrypt(
        self,
        encrypted: EncryptedData,
        *,
        context: dict[str, str] | None = None,
    ) -> bytes:
        """Decrypt data.

        Args:
            encrypted: EncryptedData to decrypt
            context: Override context (uses encrypted.context if not provided)

        Returns:
            Decrypted plaintext bytes

        Raises:
            DecryptionError: If decryption fails
        """
        ctx = context or encrypted.context
        key_id = encrypted.key_reference.key_id
        logger.debug("Decrypting data with key %s", key_id)

        try:
            return await self._do_decrypt(encrypted.ciphertext, key_id, ctx)
        except DecryptionError:
            raise
        except Exception as e:
            # SECURITY: Log full error for debugging, but sanitize message to caller
            logger.exception("Decryption failed for key %s", key_id)
            raise DecryptionError("Decryption operation failed") from e

    async def generate_data_key(
        self,
        key_id: str,
        *,
        context: dict[str, str] | None = None,
    ) -> DataKeyPair:
        """Generate a data encryption key (envelope encryption).

        Args:
            key_id: Master key to wrap the data key
            context: Additional authenticated data

        Returns:
            DataKeyPair with plaintext and encrypted data key
        """
        ctx = context or {}
        logger.debug("Generating data key with master key %s", key_id)

        try:
            plaintext_key, encrypted_key_bytes = await self._do_generate_data_key(key_id, ctx)

            key_ref = KeyReference(
                provider=self.provider_name,
                key_id=key_id,
                key_alias=None,
                purpose=KeyPurpose.ENCRYPT_DECRYPT,
                created_at=datetime.now(UTC),
            )

            return DataKeyPair(
                plaintext_key=plaintext_key,
                encrypted_key=EncryptedData(
                    ciphertext=encrypted_key_bytes,
                    key_reference=key_ref,
                    algorithm="data-key",
                    encrypted_at=datetime.now(UTC),
                    context=ctx,
                ),
                key_reference=key_ref,
            )
        except Exception as e:
            # SECURITY: Log full error, sanitize message to caller
            logger.exception("Data key generation failed for key %s", key_id)
            raise EncryptionError("Data key generation failed") from e

    async def rotate_key(self, key_id: str) -> KeyReference:
        """Rotate the specified key."""
        logger.info("Rotating key %s", key_id)
        try:
            await self._do_rotate_key(key_id)
            return await self.get_key_info(key_id)
        except Exception as e:
            # SECURITY: Log full error, sanitize message to caller
            logger.exception("Key rotation failed for key %s", key_id)
            raise KeyRotationError("Key rotation failed") from e

    async def get_key_info(self, key_id: str) -> KeyReference:
        """Get key metadata."""
        info = await self._do_get_key_info(key_id)
        return KeyReference(
            provider=self.provider_name,
            key_id=key_id,
            key_alias=info.get("alias"),
            purpose=KeyPurpose.ENCRYPT_DECRYPT,
            created_at=info.get("created_at", datetime.now(UTC)),
        )

    async def get_rotation_policy(self, key_id: str) -> RotationPolicy:
        """Get key rotation policy."""
        policy = await self._do_get_rotation_policy(key_id)
        return RotationPolicy(
            enabled=policy.get("enabled", False),
            rotation_period_days=policy.get("period_days", 0),
            last_rotation=policy.get("last_rotation"),
            next_rotation=policy.get("next_rotation"),
        )

    # ─────────────────────────────────────────────────────────────
    # Abstract Methods (provider-specific hooks)
    # ─────────────────────────────────────────────────────────────

    @abstractmethod
    async def _do_encrypt(
        self,
        plaintext: bytes,
        key_id: str,
        context: dict[str, str],
    ) -> tuple[bytes, str]:
        """Provider-specific encryption.

        Args:
            plaintext: Data to encrypt
            key_id: Key identifier
            context: AAD context (never None)

        Returns:
            Tuple of (ciphertext, algorithm_name)
        """
        ...

    @abstractmethod
    async def _do_decrypt(
        self,
        ciphertext: bytes,
        key_id: str,
        context: dict[str, str],
    ) -> bytes:
        """Provider-specific decryption.

        Args:
            ciphertext: Data to decrypt
            key_id: Key identifier
            context: AAD context (never None)

        Returns:
            Decrypted plaintext
        """
        ...

    @abstractmethod
    async def _do_generate_data_key(
        self,
        key_id: str,
        context: dict[str, str],
    ) -> tuple[bytes, bytes]:
        """Provider-specific data key generation.

        Args:
            key_id: Master key identifier
            context: AAD context (never None)

        Returns:
            Tuple of (plaintext_key, encrypted_key)
        """
        ...

    @abstractmethod
    async def _do_rotate_key(self, key_id: str) -> None:
        """Provider-specific key rotation."""
        ...

    @abstractmethod
    async def _do_get_key_info(self, key_id: str) -> dict:
        """Provider-specific key info retrieval.

        Returns:
            Dict with 'alias' (str|None) and 'created_at' (datetime)
        """
        ...

    @abstractmethod
    async def _do_get_rotation_policy(self, key_id: str) -> dict:
        """Provider-specific rotation policy retrieval.

        Returns:
            Dict with 'enabled' (bool), 'period_days' (int),
            'last_rotation' (datetime|None), 'next_rotation' (datetime|None)
        """
        ...
