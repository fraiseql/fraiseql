"""KMS domain models.

Value objects for key management operations.

Algorithm Naming Convention:
---------------------------
Each provider returns its native algorithm identifier in EncryptedData.algorithm:

| Provider | Algorithm String              | Actual Algorithm        |
|----------|------------------------------|-------------------------|
| Vault    | "aes256-gcm96"               | AES-256-GCM (96-bit IV) |
| AWS      | "SYMMETRIC_DEFAULT"          | AES-256-GCM             |
| GCP      | "GOOGLE_SYMMETRIC_ENCRYPTION"| AES-256-GCM             |

NOTE: Algorithm strings are provider-scoped. Do not compare algorithms across
providers directly. If you need to verify algorithm compatibility, check against
known values for the specific provider in key_reference.provider.

All three providers use AES-256-GCM under the hood, but their naming differs.
"""

from dataclasses import dataclass
from datetime import datetime
from enum import Enum
from typing import Any


class KeyPurpose(Enum):
    """Intended use of the key."""

    ENCRYPT_DECRYPT = "encrypt_decrypt"
    SIGN_VERIFY = "sign_verify"
    MAC = "mac"


class KeyState(Enum):
    """Current state of the key."""

    ENABLED = "enabled"
    DISABLED = "disabled"
    PENDING_DELETION = "pending_deletion"
    DESTROYED = "destroyed"


@dataclass(frozen=True)
class KeyReference:
    """Immutable reference to a key in KMS.

    Attributes:
        provider: Provider identifier (e.g., 'vault', 'aws', 'gcp')
        key_id: Provider-specific key identifier
        key_alias: Human-readable alias (optional)
        purpose: Intended use of the key
        created_at: When the key was created
    """

    provider: str
    key_id: str
    key_alias: str | None
    purpose: KeyPurpose
    created_at: datetime

    @property
    def qualified_id(self) -> str:
        """Fully qualified key identifier."""
        return f"{self.provider}:{self.key_id}"


@dataclass(frozen=True)
class EncryptedData:
    """Encrypted data with metadata.

    Attributes:
        ciphertext: The encrypted bytes
        key_reference: Reference to the key used
        algorithm: Encryption algorithm used
        encrypted_at: When encryption occurred
        context: Additional authenticated data (AAD)
    """

    ciphertext: bytes
    key_reference: KeyReference
    algorithm: str
    encrypted_at: datetime
    context: dict[str, str]

    def to_dict(self) -> dict[str, Any]:
        """Serialize for storage."""
        return {
            "ciphertext": self.ciphertext.hex(),
            "key_id": self.key_reference.qualified_id,
            "algorithm": self.algorithm,
            "encrypted_at": self.encrypted_at.isoformat(),
            "context": self.context,
        }


@dataclass(frozen=True)
class DataKeyPair:
    """Data key pair for envelope encryption.

    Attributes:
        plaintext_key: Use immediately, never persist
        encrypted_key: Persist alongside encrypted data
        key_reference: Master key used for wrapping
    """

    plaintext_key: bytes
    encrypted_key: EncryptedData
    key_reference: KeyReference


@dataclass(frozen=True)
class RotationPolicy:
    """Key rotation configuration.

    Attributes:
        enabled: Whether automatic rotation is enabled
        rotation_period_days: Days between rotations
        last_rotation: When key was last rotated
        next_rotation: When key will next be rotated
    """

    enabled: bool
    rotation_period_days: int
    last_rotation: datetime | None
    next_rotation: datetime | None
