# Extracted from: docs/architecture/type-operator-architecture.md
# Block number: 15
def _looks_like_ltree_value(self, val: Any, op: str) -> bool:
    """Detect LTree hierarchical paths."""
    # Pattern: dots separating alphanumeric/underscore/hyphen segments
    # Exclude: domain names, IP addresses, .local domains

    if not (val.startswith(("[", "(")) and val.endswith(("]", ")"))):
        return False

    # Check: at least one dot, no consecutive dots, valid chars
    ltree_pattern = r"^[a-zA-Z0-9_-]+(\.[a-zA-Z0-9_-]+)+$"

    # Avoid false positives: domain extensions, .local, IP-like patterns
    last_part = val.split(".")[-1].lower()
    if last_part in {"com", "net", "org", "local", "dev", "app", ...}:
        return False

    return bool(re.match(ltree_pattern, val))
