"""KMS domain exceptions."""


class KMSError(Exception):
    """Base exception for all KMS operations.

    This is the root exception class for all Key Management Service related errors.
    All KMS-specific exceptions inherit from this class to allow for unified error
    handling and logging.
    """


class KeyNotFoundError(KMSError):
    """Raised when a requested key cannot be found in the KMS provider.

    This exception is raised when attempting to perform operations on a key that
    does not exist in the specified KMS provider (Vault, AWS KMS, GCP Cloud KMS, etc.).

    Common causes:
    - Incorrect key ID or alias
    - Key was deleted or rotated
    - Insufficient permissions to access the key
    """


class EncryptionError(KMSError):
    """Raised when data encryption fails.

    This exception is raised when an encryption operation cannot be completed.
    This may be due to cryptographic errors, provider connectivity issues,
    or invalid input parameters.

    Common causes:
    - Invalid plaintext data
    - Key not suitable for encryption
    - Provider service unavailable
    - Insufficient permissions
    """


class DecryptionError(KMSError):
    """Raised when data decryption fails.

    This exception is raised when a decryption operation cannot be completed.
    This may indicate data corruption, incorrect keys, or authentication failures.

    Common causes:
    - Corrupted ciphertext
    - Wrong decryption key
    - Modified authentication data (AAD)
    - Provider service unavailable
    """


class KeyRotationError(KMSError):
    """Raised when key rotation fails.

    This exception is raised when attempting to rotate a key in the KMS provider
    but the operation cannot be completed.

    Common causes:
    - Key not eligible for rotation
    - Insufficient permissions
    - Provider service unavailable
    - Rotation policy restrictions
    """


class ProviderConnectionError(KMSError):
    """Raised when connection to KMS provider fails.

    This exception is raised when the application cannot establish or maintain
    a connection to the KMS provider service.

    Common causes:
    - Network connectivity issues
    - Invalid provider configuration
    - Provider service down or unreachable
    - Authentication/authorization failures
    """
