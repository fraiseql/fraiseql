"""KMS domain layer."""

from fraiseql.security.kms.domain.base import BaseKMSProvider
from fraiseql.security.kms.domain.exceptions import (
    DecryptionError,
    EncryptionError,
    KeyNotFoundError,
    KeyRotationError,
    KMSError,
    ProviderConnectionError,
)
from fraiseql.security.kms.domain.models import (
    DataKeyPair,
    EncryptedData,
    KeyPurpose,
    KeyReference,
    KeyState,
    RotationPolicy,
)

__all__ = [
    "BaseKMSProvider",
    "DataKeyPair",
    "DecryptionError",
    "EncryptedData",
    "EncryptionError",
    "KMSError",
    "KeyNotFoundError",
    "KeyPurpose",
    "KeyReference",
    "KeyRotationError",
    "KeyState",
    "ProviderConnectionError",
    "RotationPolicy",
]
