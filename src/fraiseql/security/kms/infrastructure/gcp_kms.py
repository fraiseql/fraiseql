"""GCP Cloud KMS provider."""

import json
import secrets
from dataclasses import dataclass
from typing import TYPE_CHECKING, Any

from fraiseql.security.kms.domain.base import BaseKMSProvider
from fraiseql.security.kms.domain.exceptions import KeyNotFoundError

if TYPE_CHECKING:
    from google.api_core import exceptions as gcp_exceptions
    from google.cloud import kms_v1
    from google.cloud.kms_v1 import types

try:
    from google.api_core import exceptions as gcp_exceptions
    from google.cloud import kms_v1
    from google.cloud.kms_v1 import types

    GCP_KMS_AVAILABLE = True
except ImportError:
    GCP_KMS_AVAILABLE = False
    kms_v1 = None  # type: ignore[assignment]
    types = None  # type: ignore[assignment]
    gcp_exceptions = None  # type: ignore[assignment]


@dataclass
class GCPKMSConfig:
    """Configuration for GCP Cloud KMS provider."""

    project_id: str
    location: str
    key_ring: str

    def key_path(self, key_id: str) -> str:
        """Build full key resource path."""
        return (
            f"projects/{self.project_id}/"
            f"locations/{self.location}/"
            f"keyRings/{self.key_ring}/"
            f"cryptoKeys/{key_id}"
        )

    def key_version_path(self, key_id: str, version: str) -> str:
        """Build key version resource path."""
        return f"{self.key_path(key_id)}/cryptoKeyVersions/{version}"


class GCPKMSProvider(BaseKMSProvider):
    """GCP Cloud KMS implementation.

    Extends BaseKMSProvider - only implements the _do_* hooks.

    SECURITY CONSIDERATION - Data Key Generation:
    ---------------------------------------------
    Unlike AWS KMS (GenerateDataKey) and Vault Transit (datakey/plaintext),
    GCP Cloud KMS does not provide a native data key generation API.

    This provider implements envelope encryption by:
    1. Generating a random 32-byte key LOCALLY using secrets.token_bytes()
    2. Encrypting that key with the GCP master key
    3. Returning both the plaintext and encrypted key

    Security Implications:
    - The plaintext key briefly exists in local memory before encryption
    - AWS/Vault generate the key server-side, so plaintext never leaves KMS
    - For GCP, ensure the machine running this code is trusted
    - Consider using GCP's ImportJob for importing pre-generated keys
      if your threat model requires server-side key generation

    The local generation uses Python's `secrets` module which provides
    cryptographically secure random bytes suitable for key material.
    """

    def __init__(self, config: GCPKMSConfig) -> None:
        if not GCP_KMS_AVAILABLE:
            raise ImportError("google-cloud-kms required: pip install 'fraiseql[kms-gcp]'")
        self._config = config
        self._client = None

    async def _get_client(self) -> Any:
        """Lazy initialization of async client."""
        if not GCP_KMS_AVAILABLE or kms_v1 is None:
            raise ImportError("google-cloud-kms not available")

        if self._client is None:
            self._client = kms_v1.KeyManagementServiceAsyncClient()
        return self._client

    @property
    def provider_name(self) -> str:
        """Provider identifier."""
        return "gcp"

    async def _do_encrypt(
        self,
        plaintext: bytes,
        key_id: str,
        context: dict[str, str],
    ) -> tuple[bytes, str]:
        """Encrypt using GCP Cloud KMS."""
        if not GCP_KMS_AVAILABLE or kms_v1 is None or types is None or gcp_exceptions is None:
            raise ImportError("google-cloud-kms not available")

        client = await self._get_client()
        aad = json.dumps(context, sort_keys=True).encode() if context else None

        request = types.EncryptRequest(
            name=self._config.key_path(key_id),
            plaintext=plaintext,
            additional_authenticated_data=aad,
        )

        try:
            response = await client.encrypt(request=request)
            return response.ciphertext, "GOOGLE_SYMMETRIC_ENCRYPTION"
        except gcp_exceptions.NotFound:
            raise KeyNotFoundError(f"Key not found: {key_id}")

    async def _do_decrypt(
        self,
        ciphertext: bytes,
        key_id: str,
        context: dict[str, str],
    ) -> bytes:
        """Decrypt using GCP Cloud KMS."""
        if not GCP_KMS_AVAILABLE or kms_v1 is None or types is None or gcp_exceptions is None:
            raise ImportError("google-cloud-kms not available")

        client = await self._get_client()
        aad = json.dumps(context, sort_keys=True).encode() if context else None

        request = types.DecryptRequest(
            name=self._config.key_path(key_id),
            ciphertext=ciphertext,
            additional_authenticated_data=aad,
        )

        try:
            response = await client.decrypt(request=request)
            return response.plaintext
        except gcp_exceptions.NotFound:
            raise KeyNotFoundError(f"Key not found: {key_id}")

    async def _do_generate_data_key(
        self,
        key_id: str,
        context: dict[str, str],
    ) -> tuple[bytes, bytes]:
        """Manual envelope encryption (GCP doesn't have native data key gen).

        Generates a 32-byte AES-256 key locally and encrypts it with the
        master key. See class docstring for security considerations.
        """
        plaintext_key = secrets.token_bytes(32)  # AES-256
        ciphertext, _ = await self._do_encrypt(plaintext_key, key_id, context)
        return plaintext_key, ciphertext

    async def _do_rotate_key(self, key_id: str) -> None:
        """Create new key version in GCP Cloud KMS."""
        if not GCP_KMS_AVAILABLE or kms_v1 is None or types is None:
            raise ImportError("google-cloud-kms not available")

        client = await self._get_client()
        request = types.CreateCryptoKeyVersionRequest(parent=self._config.key_path(key_id))
        await client.create_crypto_key_version(request=request)

    async def _do_get_key_info(self, key_id: str) -> dict:
        """Get key info from GCP Cloud KMS."""
        if not GCP_KMS_AVAILABLE or kms_v1 is None or types is None or gcp_exceptions is None:
            raise ImportError("google-cloud-kms not available")

        client = await self._get_client()
        request = types.GetCryptoKeyRequest(name=self._config.key_path(key_id))

        try:
            response = await client.get_crypto_key(request=request)
            return {
                "alias": response.name,
                "created_at": response.create_time,
            }
        except gcp_exceptions.NotFound:
            raise KeyNotFoundError(f"Key not found: {key_id}")

    async def _do_get_rotation_policy(self, key_id: str) -> dict:
        """Get rotation policy from GCP Cloud KMS."""
        if not GCP_KMS_AVAILABLE or kms_v1 is None or types is None or gcp_exceptions is None:
            raise ImportError("google-cloud-kms not available")

        client = await self._get_client()
        request = types.GetCryptoKeyRequest(name=self._config.key_path(key_id))

        try:
            response = await client.get_crypto_key(request=request)
            period = response.rotation_period
            enabled = period is not None and period.seconds > 0
            return {
                "enabled": enabled,
                "period_days": period.seconds // 86400 if enabled else 0,
                "last_rotation": (response.primary.create_time if response.primary else None),
                "next_rotation": response.next_rotation_time,
            }
        except gcp_exceptions.NotFound:
            raise KeyNotFoundError(f"Key not found: {key_id}")
