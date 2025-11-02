# Extracted from: docs/architecture/type-operator-architecture.md
# Block number: 14
def _looks_like_mac_address_value(self, val: Any, op: str) -> bool:
    """Detect MAC addresses."""
    mac_clean = val.replace(":", "").replace("-", "").replace(" ", "").upper()

    # MAC is exactly 12 hex characters
    if len(mac_clean) == 12 and all(c in "0123456789ABCDEF" for c in mac_clean):
        return True

    return False
