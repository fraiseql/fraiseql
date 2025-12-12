"""HashiCorp Vault Transit secrets engine provider."""

import base64
import json
from dataclasses import dataclass
from typing import Any

import httpx

from fraiseql.security.kms.domain.base import BaseKMSProvider
from fraiseql.security.kms.domain.exceptions import KeyNotFoundError


@dataclass
class VaultConfig:
    """Configuration for Vault KMS provider.

    SECURITY CONSIDERATIONS:
    ------------------------
    Token Handling:
    - The Vault token is stored in memory for the provider's lifetime
    - Python cannot securely erase memory, so tokens may persist until GC
    - For production deployments, consider:
      1. Using short-lived tokens with automatic renewal
      2. Vault Agent with auto-auth for token management
      3. AppRole authentication with response wrapping
      4. Kubernetes auth method in K8s environments

    Recommended Production Setup:
    - Run Vault Agent as a sidecar that handles authentication
    - Configure token_file to read from Agent's sink file
    - Enable token renewal in Vault Agent config
    - Use response wrapping for initial token delivery
    """

    vault_addr: str
    token: str
    mount_path: str = "transit"
    namespace: str | None = None
    verify_tls: bool = True
    timeout: float = 30.0
    # Future: Support token file for Vault Agent integration
    # token_file: str | None = None

    def api_url(self, path: str) -> str:
        """Build full API URL for a path."""
        addr = self.vault_addr.rstrip("/")
        return f"{addr}/v1/{self.mount_path}/{path}"

    @property
    def headers(self) -> dict[str, str]:
        """Build request headers."""
        headers = {"X-Vault-Token": self.token}
        if self.namespace:
            headers["X-Vault-Namespace"] = self.namespace
        return headers


class VaultKMSProvider(BaseKMSProvider):
    """HashiCorp Vault Transit secrets engine provider.

    Uses Vault's Transit secrets engine for encryption/decryption operations.
    Supports envelope encryption via data key generation.

    SECURITY: All operations use authenticated encryption (AES-256-GCM).
    """

    def __init__(self, config: VaultConfig) -> None:
        self.config = config

    @property
    def provider_name(self) -> str:
        """Provider identifier."""
        return "vault"

    async def _do_encrypt(
        self,
        plaintext: bytes,
        key_id: str,
        context: dict[str, str],
    ) -> tuple[bytes, str]:
        """Encrypt data using Vault Transit engine."""
        url = self.config.api_url(f"encrypt/{key_id}")
        payload = {
            "plaintext": base64.b64encode(plaintext).decode(),
        }

        # Add context if provided (used for key derivation)
        if context:
            payload["context"] = base64.b64encode(
                json.dumps(context, sort_keys=True).encode()
            ).decode()

        async with httpx.AsyncClient(
            verify=self.config.verify_tls,
            timeout=self.config.timeout,
        ) as client:
            response = await client.post(
                url,
                headers=self.config.headers,
                json=payload,
            )
            response.raise_for_status()

            data = response.json()["data"]
            return data["ciphertext"].encode(), "aes256-gcm96"

    async def _do_decrypt(
        self,
        ciphertext: bytes,
        key_id: str,
        context: dict[str, str],
    ) -> bytes:
        """Decrypt data using Vault Transit engine."""
        url = self.config.api_url(f"decrypt/{key_id}")
        payload = {
            "ciphertext": ciphertext.decode(),
        }

        # Add context if provided
        if context:
            payload["context"] = base64.b64encode(
                json.dumps(context, sort_keys=True).encode()
            ).decode()

        async with httpx.AsyncClient(
            verify=self.config.verify_tls,
            timeout=self.config.timeout,
        ) as client:
            response = await client.post(
                url,
                headers=self.config.headers,
                json=payload,
            )
            response.raise_for_status()

            data = response.json()["data"]
            return base64.b64decode(data["plaintext"])

    async def _do_generate_data_key(
        self,
        key_id: str,
        context: dict[str, str],
    ) -> tuple[bytes, bytes]:
        """Generate a data encryption key using Vault."""
        url = self.config.api_url(f"datakey/plaintext/{key_id}")
        payload: dict[str, Any] = {
            "bits": 256,  # AES-256
        }

        # Add context if provided
        if context:
            payload["context"] = base64.b64encode(
                json.dumps(context, sort_keys=True).encode()
            ).decode()

        async with httpx.AsyncClient(
            verify=self.config.verify_tls,
            timeout=self.config.timeout,
        ) as client:
            response = await client.post(
                url,
                headers=self.config.headers,
                json=payload,
            )
            response.raise_for_status()

            data = response.json()["data"]
            plaintext_key = base64.b64decode(data["plaintext"])
            # Vault returns ciphertext in its format (vault:v1:base64data), not raw base64
            # We store it as-is (encoded as bytes) since encrypt() returns it the same way
            encrypted_key = data["ciphertext"].encode()

            return plaintext_key, encrypted_key

    async def _do_rotate_key(self, key_id: str) -> None:
        """Rotate the key in Vault."""
        url = self.config.api_url(f"keys/{key_id}/rotate")

        async with httpx.AsyncClient(
            verify=self.config.verify_tls,
            timeout=self.config.timeout,
        ) as client:
            response = await client.post(
                url,
                headers=self.config.headers,
                json={},
            )
            response.raise_for_status()

    async def _do_get_key_info(self, key_id: str) -> dict:
        """Get key information from Vault."""
        url = self.config.api_url(f"keys/{key_id}")

        async with httpx.AsyncClient(
            verify=self.config.verify_tls,
            timeout=self.config.timeout,
        ) as client:
            response = await client.get(
                url,
                headers=self.config.headers,
            )

            if response.status_code == 404:
                raise KeyNotFoundError(f"Key {key_id} not found in Vault")

            response.raise_for_status()
            data = response.json()["data"]

            return {
                "alias": data.get("name"),  # Vault uses 'name' for alias
                "created_at": data.get("created_time"),
            }

    async def _do_get_rotation_policy(self, key_id: str) -> dict:
        """Get key rotation policy from Vault."""
        url = self.config.api_url(f"keys/{key_id}")

        async with httpx.AsyncClient(
            verify=self.config.verify_tls,
            timeout=self.config.timeout,
        ) as client:
            response = await client.get(
                url,
                headers=self.config.headers,
            )

            if response.status_code == 404:
                raise KeyNotFoundError(f"Key {key_id} not found in Vault")

            response.raise_for_status()
            data = response.json()["data"]

            # Vault doesn't have explicit rotation policies in transit engine
            # Return disabled by default
            return {
                "enabled": False,
                "period_days": 0,
                "last_rotation": data.get("latest_version"),
                "next_rotation": None,
            }
