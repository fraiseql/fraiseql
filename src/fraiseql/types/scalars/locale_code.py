"""Locale code scalar type for BCP 47 validation."""

import re
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode

from fraiseql.types.definitions import ScalarMarker

# BCP 47: language-REGION format (en-US, fr-FR, de-DE, etc.)
# Also supports language-only (en, fr) for flexibility
_LOCALE_CODE_REGEX = re.compile(r"^[a-z]{2}(-[A-Z]{2})?$")


def serialize_locale_code(value: Any) -> str | None:
    """Serialize locale code to string."""
    if value is None:
        return None

    value_str = str(value)

    if not _LOCALE_CODE_REGEX.match(value_str):
        raise GraphQLError(
            f"Invalid locale code: {value}. Must be BCP 47 format (e.g., 'en-US', 'fr-FR', 'de-DE')"
        )

    return value_str


def parse_locale_code_value(value: Any) -> str:
    """Parse locale code from variable value."""
    if not isinstance(value, str):
        raise GraphQLError(f"Locale code must be a string, got {type(value).__name__}")

    if not _LOCALE_CODE_REGEX.match(value):
        raise GraphQLError(
            f"Invalid locale code: {value}. Must be BCP 47 format (e.g., 'en-US', 'fr-FR', 'de-DE')"
        )

    return value


def parse_locale_code_literal(ast: Any, _variables: dict[str, Any] | None = None) -> str:
    """Parse locale code from AST literal."""
    if not isinstance(ast, StringValueNode):
        raise GraphQLError("Locale code must be a string")

    return parse_locale_code_value(ast.value)


LocaleCodeScalar = GraphQLScalarType(
    name="LocaleCode",
    description=(
        "BCP 47 locale code (language-REGION format). "
        "Format: lowercase language + hyphen + uppercase region. "
        "Examples: en-US, fr-FR, de-DE, es-ES, ja-JP, zh-CN. "
        "See: https://tools.ietf.org/html/bcp47"
    ),
    serialize=serialize_locale_code,
    parse_value=parse_locale_code_value,
    parse_literal=parse_locale_code_literal,
)


class LocaleCodeField(str, ScalarMarker):
    """BCP 47 locale code for regional/cultural formatting.

    This scalar validates locale codes following BCP 47 standard:
    - Format: language-REGION (e.g., en-US, fr-FR)
    - Language: 2 lowercase letters (ISO 639-1)
    - Region: 2 uppercase letters (ISO 3166-1 alpha-2)
    - Language-only also accepted (e.g., en, fr)

    Example:
        >>> from fraiseql.types import LocaleCode
        >>>
        >>> @fraiseql.type
        ... class User:
        ...     locale: LocaleCode  # for date/number formatting
        ...     language: LanguageCode  # for content translation
    """

    __slots__ = ()

    def __new__(cls, value: str) -> "LocaleCodeField":
        """Create a new LocaleCodeField instance with validation."""
        if not _LOCALE_CODE_REGEX.match(value):
            raise ValueError(
                f"Invalid locale code: {value}. Must be BCP 47 format "
                "(e.g., 'en-US', 'fr-FR', 'de-DE')"
            )
        return super().__new__(cls, value)
