"""Language code scalar type for ISO 639-1 validation."""

import re
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode

from fraiseql.types.definitions import ScalarMarker

# ISO 639-1: Two-letter language codes (en, fr, de, es, ja, etc.)
_LANGUAGE_CODE_REGEX = re.compile(r"^[a-z]{2}$")


def serialize_language_code(value: Any) -> str | None:
    """Serialize language code to string."""
    if value is None:
        return None

    value_str = str(value).lower()

    if not _LANGUAGE_CODE_REGEX.match(value_str):
        raise GraphQLError(
            f"Invalid language code: {value}. Must be ISO 639-1 two-letter code "
            "(e.g., 'en', 'fr', 'de')"
        )

    return value_str


def parse_language_code_value(value: Any) -> str:
    """Parse language code from variable value."""
    if not isinstance(value, str):
        raise GraphQLError(f"Language code must be a string, got {type(value).__name__}")

    value_lower = value.lower()

    if not _LANGUAGE_CODE_REGEX.match(value_lower):
        raise GraphQLError(
            f"Invalid language code: {value}. Must be ISO 639-1 two-letter code "
            "(e.g., 'en', 'fr', 'de')"
        )

    return value_lower


def parse_language_code_literal(ast: Any, _variables: dict[str, Any] | None = None) -> str:
    """Parse language code from AST literal."""
    if not isinstance(ast, StringValueNode):
        raise GraphQLError("Language code must be a string")

    return parse_language_code_value(ast.value)


LanguageCodeScalar = GraphQLScalarType(
    name="LanguageCode",
    description=(
        "ISO 639-1 two-letter language code. "
        "Valid codes: en, fr, de, es, ja, zh, etc. "
        "See: https://en.wikipedia.org/wiki/List_of_ISO_639-1_codes"
    ),
    serialize=serialize_language_code,
    parse_value=parse_language_code_value,
    parse_literal=parse_language_code_literal,
)


class LanguageCodeField(str, ScalarMarker):
    """ISO 639-1 two-letter language code.

    This scalar validates that the language code follows ISO 639-1 standard:
    - Exactly 2 lowercase letters
    - Valid codes: en, fr, de, es, ja, zh, ar, etc.
    - Case-insensitive (normalized to lowercase)

    Example:
        >>> from fraiseql.types import LanguageCode
        >>>
        >>> @fraiseql.input
        ... class UserPreferences:
        ...     language: LanguageCode
        ...     fallback_language: LanguageCode | None
    """

    __slots__ = ()

    def __new__(cls, value: str) -> "LanguageCodeField":
        """Create a new LanguageCodeField instance with validation."""
        value_lower = value.lower()
        if not _LANGUAGE_CODE_REGEX.match(value_lower):
            raise ValueError(
                f"Invalid language code: {value}. Must be ISO 639-1 two-letter code "
                "(e.g., 'en', 'fr', 'de')"
            )
        return super().__new__(cls, value_lower)
