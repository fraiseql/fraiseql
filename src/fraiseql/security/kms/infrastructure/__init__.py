"""KMS infrastructure providers."""

from fraiseql.security.kms.infrastructure.local import (
    LocalKMSConfig,
    LocalKMSProvider,
)
from fraiseql.security.kms.infrastructure.vault import (
    VaultConfig,
    VaultKMSProvider,
)

__all__ = [
    "LocalKMSConfig",
    "LocalKMSProvider",
    "VaultConfig",
    "VaultKMSProvider",
]

# Optional providers (may not be installed)
try:
    from fraiseql.security.kms.infrastructure.aws_kms import (
        AWSKMSConfig,
        AWSKMSProvider,
    )

    __all__ += ["AWSKMSConfig", "AWSKMSProvider"]
except ImportError:
    pass

try:
    from fraiseql.security.kms.infrastructure.gcp_kms import (
        GCPKMSConfig,
        GCPKMSProvider,
    )

    __all__ += ["GCPKMSConfig", "GCPKMSProvider"]
except ImportError:
    pass
