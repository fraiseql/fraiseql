# Extracted from: docs/architecture/type-operator-architecture.md
# Block number: 13
def _looks_like_ip_address_value(self, val: Any, op: str) -> bool:
    """Detect IP addresses (fallback when field_type missing)."""
    if isinstance(val, str):
        try:
            ipaddress.ip_address(val)  # Try parse
            return True
        except ValueError:
            try:
                ipaddress.ip_network(val, strict=False)  # Try CIDR
                return True
            except ValueError:
                pass

        # Heuristic: IPv4-like pattern
        if val.count(".") == 3 and all(0 <= int(p) <= 255 for p in val.split(".")):
            return True

        # Heuristic: IPv6-like pattern (contains hex + colons)
        if ":" in val and val.count(":") >= 2:
            return all(c in "0123456789abcdefABCDEF:" for c in val)

    return False
