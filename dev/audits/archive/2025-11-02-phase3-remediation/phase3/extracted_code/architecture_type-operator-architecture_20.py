# Extracted from: docs/architecture/type-operator-architecture.md
# Block number: 20
class FieldType(Enum):
    """Enumeration of field types for where clause generation."""

    ANY = "any"
    STRING = "string"
    INTEGER = "integer"
    IP_ADDRESS = "ip_address"
    MAC_ADDRESS = "mac_address"
    LTREE = "ltree"
    DATE_RANGE = "date_range"
    # ... more types


def detect_field_type(field_name: str, value: Any, field_type: type | None = None) -> FieldType:
    """Detect the type of field based on:
    1. Explicit type hint
    2. Field name patterns (e.g., "ip_address", "mac_address")
    3. Value analysis (heuristics)
    """
