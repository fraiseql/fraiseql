"""SHA256 hash scalar type for cryptographic hash validation."""

import re
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode

from fraiseql.types.definitions import ScalarMarker

# SHA256 hash regex: exactly 64 hexadecimal characters
_HASH_SHA256_REGEX = re.compile(r"^[a-fA-F0-9]{64}$")


def serialize_hash_sha256(value: Any) -> str | None:
    """Serialize SHA256 hash to string."""
    if value is None:
        return None

    value_str = str(value)

    if not _HASH_SHA256_REGEX.match(value_str):
        raise GraphQLError(
            f"Invalid SHA256 hash: {value}. Must be exactly 64 hexadecimal characters "
            "(e.g., 'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855')"
        )

    return value_str


def parse_hash_sha256_value(value: Any) -> str:
    """Parse SHA256 hash from variable value."""
    if not isinstance(value, str):
        raise GraphQLError(f"SHA256 hash must be a string, got {type(value).__name__}")

    if not _HASH_SHA256_REGEX.match(value):
        raise GraphQLError(
            f"Invalid SHA256 hash: {value}. Must be exactly 64 hexadecimal characters "
            "(e.g., 'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855')"
        )

    return value


def parse_hash_sha256_literal(ast: Any, _variables: dict[str, Any] | None = None) -> str:
    """Parse SHA256 hash from AST literal."""
    if not isinstance(ast, StringValueNode):
        raise GraphQLError("SHA256 hash must be a string")

    return parse_hash_sha256_value(ast.value)


HashSHA256Scalar = GraphQLScalarType(
    name="HashSHA256",
    description=(
        "SHA256 cryptographic hash. Must be exactly 64 hexadecimal characters. "
        "Examples: e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855, "
        "a665a45920422f9d417e4867efdc4fb8a04a1f3fff1fa07e998e86f7f7a27ae3"
    ),
    serialize=serialize_hash_sha256,
    parse_value=parse_hash_sha256_value,
    parse_literal=parse_hash_sha256_literal,
)


class HashSHA256Field(str, ScalarMarker):
    """SHA256 cryptographic hash.

    This scalar validates that the hash follows SHA256 format:
    - Exactly 64 characters
    - Only hexadecimal characters (0-9, a-f, A-F)
    - Case-insensitive

    Example:
        >>> from fraiseql.types import HashSHA256
        >>>
        >>> @fraiseql.input
        ... class FileHashInput:
        ...     file_path: str
        ...     sha256_hash: HashSHA256
    """

    __slots__ = ()

    def __new__(cls, value: str) -> "HashSHA256Field":
        """Create a new HashSHA256Field instance with validation."""
        if not _HASH_SHA256_REGEX.match(value):
            raise ValueError(
                f"Invalid SHA256 hash: {value}. Must be exactly 64 hexadecimal characters "
                "(e.g., 'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855')"
            )
        return super().__new__(cls, value)
