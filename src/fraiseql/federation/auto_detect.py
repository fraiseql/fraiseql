"""Python-side auto-key detection for Federation Lite.

Auto-detects entity key fields from Python class annotations using a priority-based algorithm:
1. Field named 'id' (most common, ~90% of cases)
2. Field with '_primary_key' attribute
3. Common patterns like 'uuid', 'pk', 'primary_key'
4. None - returns None and lets Rust side handle error

This enables @entity decorator to work without explicit key specification.
"""

from typing import Optional, Type


def auto_detect_key_python(cls: Type) -> Optional[str]:
    """Auto-detect key field from Python class annotations.

    Uses priority-based algorithm:
    1. Field named 'id' (most common)
    2. Field marked with _primary_key attribute
    3. Common patterns (uuid, pk, primary_key, _id)
    4. None if not found

    Args:
        cls: Python class to detect key from

    Returns:
        Key field name if detected, None otherwise

    Examples:
        >>> class User:
        ...     id: str
        ...     name: str
        >>> auto_detect_key_python(User)
        'id'

        >>> class OrgUser:
        ...     org_id: str
        ...     user_id: str
        >>> auto_detect_key_python(OrgUser)
        None
    """
    annotations = getattr(cls, "__annotations__", {})

    if not annotations:
        return None

    # Priority 1: Field named 'id' (most common)
    if "id" in annotations:
        return "id"

    # Priority 2: Common key patterns
    for field_name in ["uuid", "pk", "primary_key", "_id", "UUID"]:
        if field_name in annotations:
            return field_name

    # Priority 3: Fields marked with _primary_key
    for field_name in annotations:
        if hasattr(cls, f"__{field_name}_primary_key__"):
            return field_name

    return None


def validate_key_field(cls: Type, key: Optional[str]) -> Optional[str]:
    """Validate that key field exists in class annotations.

    Args:
        cls: Python class to validate
        key: Key field name to validate

    Returns:
        Key field name if valid, None otherwise

    Raises:
        ValueError: If key doesn't exist in class
    """
    if key is None:
        return None

    annotations = getattr(cls, "__annotations__", {})

    if key not in annotations:
        available = ", ".join(annotations.keys()) if annotations else "none"
        raise ValueError(
            f"Key field '{key}' not found in {cls.__name__}. Available fields: {available}"
        )

    return key
