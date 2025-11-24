"""KMS domain exceptions."""


class KMSError(Exception):
    """Base exception for KMS operations."""


class KeyNotFoundError(KMSError):
    """Raised when a key is not found."""


class EncryptionError(KMSError):
    """Raised when encryption fails."""


class DecryptionError(KMSError):
    """Raised when decryption fails."""


class KeyRotationError(KMSError):
    """Raised when key rotation fails."""


class ProviderConnectionError(KMSError):
    """Raised when connection to KMS provider fails."""
