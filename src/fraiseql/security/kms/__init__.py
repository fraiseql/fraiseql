"""Key Management Service (KMS) for FraiseQL.

Provides unified encryption/decryption across multiple KMS providers:
- HashiCorp Vault
- AWS KMS
- GCP Cloud KMS
- Local development provider

Usage:
    from fraiseql.security.kms import KeyManager, VaultKMSProvider, VaultConfig

    # Configure provider
    config = VaultConfig(vault_addr="http://localhost:8200", token="...")
    provider = VaultKMSProvider(config)

    # Create key manager
    key_manager = KeyManager(provider)

    # Encrypt/decrypt
    encrypted = await key_manager.encrypt(b"secret data")
    plaintext = await key_manager.decrypt(encrypted)
"""

from fraiseql.security.kms.application.key_manager import KeyManager
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
from fraiseql.security.kms.infrastructure.vault import VaultConfig, VaultKMSProvider

# Optional providers (may not be installed)
try:
    from fraiseql.security.kms.infrastructure.aws_kms import AWSKMSConfig, AWSKMSProvider
except ImportError:
    AWSKMSConfig = None  # type: ignore[assignment, misc]
    AWSKMSProvider = None  # type: ignore[assignment, misc]

try:
    from fraiseql.security.kms.infrastructure.gcp_kms import GCPKMSConfig, GCPKMSProvider
except ImportError:
    GCPKMSConfig = None  # type: ignore[assignment, misc]
    GCPKMSProvider = None  # type: ignore[assignment, misc]

try:
    from fraiseql.security.kms.infrastructure.local import LocalKMSConfig, LocalKMSProvider
except ImportError:
    LocalKMSConfig = None  # type: ignore[assignment, misc]
    LocalKMSProvider = None  # type: ignore[assignment, misc]

__all__ = [
    "BaseKMSProvider",
    "DataKeyPair",
    "DecryptionError",
    "EncryptedData",
    "EncryptionError",
    "KMSError",
    "KeyManager",
    "KeyNotFoundError",
    "KeyPurpose",
    "KeyReference",
    "KeyRotationError",
    "KeyState",
    "ProviderConnectionError",
    "RotationPolicy",
    "VaultConfig",
    "VaultKMSProvider",
]

# Add optional providers to __all__ if available
if AWSKMSConfig is not None:
    __all__ += ["AWSKMSConfig", "AWSKMSProvider"]
if GCPKMSConfig is not None:
    __all__ += ["GCPKMSConfig", "GCPKMSProvider"]
if LocalKMSConfig is not None:
    __all__ += ["LocalKMSConfig", "LocalKMSProvider"]
