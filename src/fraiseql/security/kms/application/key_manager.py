"""KeyManager application service.

Unified interface for encryption operations across multiple KMS providers.

ARCHITECTURE NOTE:
------------------
This KeyManager is designed for TWO usage patterns:

1. STARTUP-TIME (recommended for performance):
   - Call initialize() at application startup
   - Generates a data encryption key (DEK) via KMS
   - Caches DEK in memory for fast local encryption
   - Use local_encrypt() / local_decrypt() in hot paths
   - Rotate keys periodically via rotate_data_key()

2. PER-REQUEST (for infrequent high-security operations):
   - Call encrypt() / decrypt() directly
   - Each call contacts KMS (50-200ms latency)
   - Use only for secrets management, not response data

The Rust pipeline should NEVER wait on KMS calls. Use local_encrypt()
with cached keys for any encryption in the request path.
"""

import secrets

from cryptography.hazmat.primitives.ciphers.aead import AESGCM

from fraiseql.security.kms.domain.base import BaseKMSProvider
from fraiseql.security.kms.domain.models import (
    DataKeyPair,
    EncryptedData,
)


class KeyManager:
    """Application service for encryption operations.

    Provides both KMS-backed encryption (slow, secure) and
    local encryption with cached keys (fast, for hot paths).
    """

    def __init__(
        self,
        providers: dict[str, BaseKMSProvider],
        default_provider: str,
        default_key_id: str,
        context_prefix: str | None = None,
    ) -> None:
        """Initialize KeyManager.

        Args:
            providers: Map of provider name -> provider instance
            default_provider: Provider to use when not specified
            default_key_id: Key ID to use when not specified
            context_prefix: Optional prefix to add to all encryption contexts
        """
        self._providers = providers
        self._default_provider = default_provider
        self._default_key_id = default_key_id
        self._context_prefix = context_prefix

        # Cached data key for local encryption (set by initialize())
        self._data_key_pair: DataKeyPair | None = None
        self._aesgcm: AESGCM | None = None

        if default_provider not in providers:
            raise ValueError(f"Default provider '{default_provider}' not in providers")

    def get_provider(self, name: str) -> BaseKMSProvider:
        """Get a provider by name."""
        if name not in self._providers:
            raise ValueError(f"Unknown provider: {name}")
        return self._providers[name]

    def _build_context(
        self,
        context: dict[str, str] | None = None,
    ) -> dict[str, str]:
        """Build encryption context with optional prefix."""
        ctx = dict(context) if context else {}
        if self._context_prefix:
            ctx["service"] = self._context_prefix
        return ctx

    # ─────────────────────────────────────────────────────────────
    # Startup-Time Key Management (recommended)
    # ─────────────────────────────────────────────────────────────

    async def initialize(self) -> None:
        """Initialize KeyManager by generating a cached data key.

        Call this at application startup. The data key is generated
        via KMS and cached in memory for fast local encryption.
        """
        provider = self.get_provider(self._default_provider)
        self._data_key_pair = await provider.generate_data_key(
            self._default_key_id,
            context=self._build_context({"purpose": "data_encryption"}),
        )
        self._aesgcm = AESGCM(self._data_key_pair.plaintext_key)

    def has_cached_data_key(self) -> bool:
        """Check if a data key is cached."""
        return self._data_key_pair is not None

    def get_cached_data_key(self) -> bytes | None:
        """Get the cached plaintext data key (for debugging only)."""
        if self._data_key_pair is None:
            return None
        return self._data_key_pair.plaintext_key

    async def rotate_data_key(self) -> None:
        """Rotate the cached data key via KMS.

        Call this periodically (e.g., every few hours) to rotate keys.
        """
        await self.initialize()

    def local_encrypt(self, plaintext: bytes) -> bytes:
        """Encrypt data using the cached data key (NO KMS call).

        This is fast (~microseconds) and safe for use in hot paths.

        Args:
            plaintext: Data to encrypt

        Returns:
            Encrypted bytes (nonce + ciphertext)

        Raises:
            RuntimeError: If initialize() was not called
        """
        if self._aesgcm is None:
            raise RuntimeError("KeyManager not initialized. Call initialize() at startup.")

        nonce = secrets.token_bytes(12)  # 96-bit nonce for AES-GCM
        ciphertext = self._aesgcm.encrypt(nonce, plaintext, None)
        return nonce + ciphertext

    def local_decrypt(self, encrypted: bytes) -> bytes:
        """Decrypt data using the cached data key (NO KMS call).

        Args:
            encrypted: Encrypted bytes (nonce + ciphertext)

        Returns:
            Decrypted plaintext

        Raises:
            RuntimeError: If initialize() was not called
        """
        if self._aesgcm is None:
            raise RuntimeError("KeyManager not initialized. Call initialize() at startup.")

        nonce = encrypted[:12]
        ciphertext = encrypted[12:]
        return self._aesgcm.decrypt(nonce, ciphertext, None)

    # ─────────────────────────────────────────────────────────────
    # Per-Request KMS Operations (slow, use sparingly)
    # ─────────────────────────────────────────────────────────────

    async def encrypt(
        self,
        plaintext: bytes,
        *,
        key_id: str | None = None,
        provider: str | None = None,
        context: dict[str, str] | None = None,
    ) -> EncryptedData:
        """Encrypt data.

        Args:
            plaintext: Data to encrypt
            key_id: Key ID (defaults to default_key_id)
            provider: Provider name (defaults to default_provider)
            context: Additional encryption context

        Returns:
            EncryptedData with ciphertext and metadata
        """
        prov = self.get_provider(provider or self._default_provider)
        return await prov.encrypt(
            plaintext,
            key_id or self._default_key_id,
            context=self._build_context(context),
        )

    async def decrypt(
        self,
        encrypted: EncryptedData,
        *,
        context: dict[str, str] | None = None,
    ) -> bytes:
        """Decrypt data.

        Auto-detects the correct provider from EncryptedData.

        Args:
            encrypted: Data to decrypt
            context: Optional context override

        Returns:
            Decrypted plaintext
        """
        provider_name = encrypted.key_reference.provider
        prov = self.get_provider(provider_name)
        return await prov.decrypt(
            encrypted,
            context=self._build_context(context) if context else None,
        )

    async def encrypt_field(
        self,
        value: str | bytes,
        *,
        key_id: str | None = None,
        provider: str | None = None,
        context: dict[str, str] | None = None,
    ) -> EncryptedData:
        """Encrypt a field value (string or bytes).

        Convenience method that handles string encoding.
        """
        if isinstance(value, str):
            plaintext = value.encode("utf-8")
            ctx = self._build_context(context)
            ctx["_encoding"] = "utf-8"
        else:
            plaintext = value
            ctx = self._build_context(context)

        prov = self.get_provider(provider or self._default_provider)
        return await prov.encrypt(
            plaintext,
            key_id or self._default_key_id,
            context=ctx,
        )

    async def decrypt_field(
        self,
        encrypted: EncryptedData,
    ) -> str | bytes:
        """Decrypt a field value.

        Returns string if original was string (based on context), bytes otherwise.
        """
        plaintext = await self.decrypt(encrypted)
        if encrypted.context.get("_encoding") == "utf-8":
            return plaintext.decode("utf-8")
        return plaintext

    async def generate_data_key(
        self,
        *,
        key_id: str | None = None,
        provider: str | None = None,
        context: dict[str, str] | None = None,
    ) -> DataKeyPair:
        """Generate a data encryption key for envelope encryption."""
        prov = self.get_provider(provider or self._default_provider)
        return await prov.generate_data_key(
            key_id or self._default_key_id,
            context=self._build_context(context),
        )
