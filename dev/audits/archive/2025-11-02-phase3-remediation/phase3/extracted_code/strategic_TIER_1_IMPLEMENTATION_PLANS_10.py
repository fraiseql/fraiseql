# Extracted from: docs/strategic/TIER_1_IMPLEMENTATION_PLANS.md
# Block number: 10
# src/fraiseql/enterprise/crypto/signing.py

from datetime import datetime
from typing import Optional


class SigningKeyManager:
    """Manages signing keys with rotation support."""

    def __init__(self):
        self.current_key: Optional[str] = None
        self.previous_keys: list[tuple[str, datetime]] = []
        self._load_keys()

    def _load_keys(self):
        """Load signing keys from environment or key vault."""
        self.current_key = os.getenv("AUDIT_SIGNING_KEY")
        if not self.current_key:
            raise ValueError("AUDIT_SIGNING_KEY environment variable not set")

    def sign(self, event_hash: str) -> str:
        """Sign event hash with current key."""
        if not self.current_key:
            raise ValueError("No signing key available")
        return sign_event(event_hash, self.current_key)

    def verify(self, event_hash: str, signature: str) -> bool:
        """Verify signature with current or previous keys."""
        # Try current key first
        if self.current_key and verify_signature(event_hash, signature, self.current_key):
            return True

        # Try previous keys (for events signed before rotation)
        for key, rotated_at in self.previous_keys:
            if verify_signature(event_hash, signature, key):
                return True

        return False


# Singleton instance
_key_manager: Optional[SigningKeyManager] = None


def get_key_manager() -> SigningKeyManager:
    """Get or create signing key manager singleton."""
    global _key_manager
    if _key_manager is None:
        _key_manager = SigningKeyManager()
    return _key_manager
