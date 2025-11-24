"""Local development KMS provider.

WARNING: This provider is for LOCAL DEVELOPMENT ONLY.
DO NOT use in production - it provides no real security.
"""

import secrets
import warnings
from dataclasses import dataclass, field
from datetime import UTC, datetime

from cryptography.hazmat.primitives.ciphers.aead import AESGCM

from fraiseql.security.kms.domain.base import BaseKMSProvider


@dataclass
class LocalKMSConfig:
    """Configuration for local development KMS.

    WARNING: For development/testing only. Not secure for production.
    """

    master_key: bytes = field(default_factory=lambda: secrets.token_bytes(32))


class LocalKMSProvider(BaseKMSProvider):
    """Local development KMS provider using in-memory keys.

    WARNING: This provider is for LOCAL DEVELOPMENT AND TESTING ONLY.
    It provides NO real security guarantees:
    - Keys are stored in memory (not HSM-protected)
    - No audit logging
    - No key rotation policies
    - No access controls

    Use VaultKMSProvider, AWSKMSProvider, or GCPKMSProvider for production.
    """

    def __init__(self, config: LocalKMSConfig | None = None) -> None:
        warnings.warn(
            "LocalKMSProvider is for development only. Use Vault/AWS/GCP providers in production.",
            UserWarning,
            stacklevel=2,
        )
        self._config = config or LocalKMSConfig()
        self._aesgcm = AESGCM(self._config.master_key)
        self._keys: dict[str, bytes] = {}

    @property
    def provider_name(self) -> str:
        """Return the provider name."""
        return "local"

    async def _do_encrypt(
        self,
        plaintext: bytes,
        key_id: str,
        context: dict[str, str],
    ) -> tuple[bytes, str]:
        """Encrypt using local AES-GCM."""
        nonce = secrets.token_bytes(12)
        aad = str(context).encode() if context else None
        ciphertext = self._aesgcm.encrypt(nonce, plaintext, aad)
        return nonce + ciphertext, "AES-256-GCM"

    async def _do_decrypt(
        self,
        ciphertext: bytes,
        key_id: str,
        context: dict[str, str],
    ) -> bytes:
        """Decrypt using local AES-GCM."""
        nonce = ciphertext[:12]
        ct = ciphertext[12:]
        aad = str(context).encode() if context else None
        return self._aesgcm.decrypt(nonce, ct, aad)

    async def _do_generate_data_key(
        self,
        key_id: str,
        context: dict[str, str],
    ) -> tuple[bytes, bytes]:
        """Generate a local data key."""
        plaintext_key = secrets.token_bytes(32)
        encrypted_key, _ = await self._do_encrypt(plaintext_key, key_id, context)
        return plaintext_key, encrypted_key

    async def _do_rotate_key(self, key_id: str) -> None:
        """No-op for local provider."""

    async def _do_get_key_info(self, key_id: str) -> dict:
        """Return mock key info."""
        return {"alias": key_id, "created_at": datetime.now(UTC)}

    async def _do_get_rotation_policy(self, key_id: str) -> dict:
        """Return mock rotation policy."""
        return {
            "enabled": False,
            "period_days": 0,
            "last_rotation": None,
            "next_rotation": None,
        }
