"""Semantic version scalar type for semver validation."""

import re
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode

from fraiseql.types.definitions import ScalarMarker

# Semantic versioning regex (semver) - MAJOR.MINOR.PATCH[-prerelease][+build]
_SEMVER_REGEX = re.compile(
    r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)"
    r"(?:-((?:0|[1-9]\d*|\d*[a-z-][0-9a-z-]*)"
    r"(?:\.(?:0|[1-9]\d*|\d*[a-z-][0-9a-z-]*))*))?"
    r"(?:\+([0-9a-z-]+(?:\.[0-9a-z-]+)*))?$"
)


def serialize_semantic_version(value: Any) -> str | None:
    """Serialize semantic version to string."""
    if value is None:
        return None

    value_str = str(value)

    if not _SEMVER_REGEX.match(value_str):
        raise GraphQLError(
            f"Invalid semantic version: {value}. Must follow semver format "
            "(e.g., '1.0.0', '2.3.4-alpha.1', '3.0.0-beta+20130313144700')"
        )

    return value_str


def parse_semantic_version_value(value: Any) -> str:
    """Parse semantic version from variable value."""
    if not isinstance(value, str):
        raise GraphQLError(f"Semantic version must be a string, got {type(value).__name__}")

    if not _SEMVER_REGEX.match(value):
        raise GraphQLError(
            f"Invalid semantic version: {value}. Must follow semver format "
            "(e.g., '1.0.0', '2.3.4-alpha.1', '3.0.0-beta+20130313144700')"
        )

    return value


def parse_semantic_version_literal(ast: Any, _variables: dict[str, Any] | None = None) -> str:
    """Parse semantic version from AST literal."""
    if not isinstance(ast, StringValueNode):
        raise GraphQLError("Semantic version must be a string")

    return parse_semantic_version_value(ast.value)


SemanticVersionScalar = GraphQLScalarType(
    name="SemanticVersion",
    description=(
        "Semantic version following semver specification. "
        "Format: MAJOR.MINOR.PATCH[-prerelease][+build]. "
        "Examples: 1.0.0, 2.3.4-alpha.1, 3.0.0-beta+20130313144700. "
        "See: https://semver.org/"
    ),
    serialize=serialize_semantic_version,
    parse_value=parse_semantic_version_value,
    parse_literal=parse_semantic_version_literal,
)


class SemanticVersionField(str, ScalarMarker):
    """Semantic version following semver specification.

    This scalar validates that the version follows semantic versioning:
    - Format: MAJOR.MINOR.PATCH[-prerelease][+build]
    - MAJOR, MINOR, PATCH: non-negative integers
    - Prerelease: optional, dot-separated identifiers
    - Build: optional, dot-separated identifiers

    Example:
        >>> from fraiseql.types import SemanticVersion
        >>>
        >>> @fraiseql.input
        ... class VersionInput:
        ...     version: SemanticVersion
        ...     api_level: int
    """

    __slots__ = ()

    def __new__(cls, value: str) -> "SemanticVersionField":
        """Create a new SemanticVersionField instance with validation."""
        if not _SEMVER_REGEX.match(value):
            raise ValueError(
                f"Invalid semantic version: {value}. Must follow semver format "
                "(e.g., '1.0.0', '2.3.4-alpha.1', '3.0.0-beta+20130313144700')"
            )
        return super().__new__(cls, value)
