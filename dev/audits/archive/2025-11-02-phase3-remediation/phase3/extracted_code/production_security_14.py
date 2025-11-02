# Extracted from: docs/production/security.md
# Block number: 14
import os

from cryptography.fernet import Fernet


class FieldEncryption:
    """Encrypt sensitive database fields."""

    def __init__(self):
        key = os.getenv("ENCRYPTION_KEY")  # Store in secrets manager
        self.cipher = Fernet(key.encode())

    def encrypt(self, value: str) -> str:
        """Encrypt field value."""
        return self.cipher.encrypt(value.encode()).decode()

    def decrypt(self, encrypted: str) -> str:
        """Decrypt field value."""
        return self.cipher.decrypt(encrypted.encode()).decode()


# Usage
encryptor = FieldEncryption()

# Store encrypted
encrypted_ssn = encryptor.encrypt("123-45-6789")
await conn.execute("INSERT INTO users (id, ssn_encrypted) VALUES ($1, $2)", user_id, encrypted_ssn)

# Retrieve and decrypt
result = await conn.execute("SELECT ssn_encrypted FROM users WHERE id = $1", user_id)
encrypted = result.fetchone()["ssn_encrypted"]
ssn = encryptor.decrypt(encrypted)
