"""FraiseQL v2 - Python Schema Authoring.

This module provides decorators for defining GraphQL schemas that are compiled
by the FraiseQL Rust engine. NO runtime FFI - decorators output JSON only.

Architecture:
    Python @decorators → schema.json → fraiseql-cli → schema.compiled.json → Rust runtime

Example:
    ```python
    import fraiseql
    from fraiseql.scalars import ID, DateTime
    from enum import Enum

    @fraiseql.enum
    class OrderStatus(Enum):
        PENDING = "pending"
        SHIPPED = "shipped"

    @fraiseql.type
    class User:
        id: ID  # UUID v4 - FraiseQL convention
        name: str
        email: str
        created_at: DateTime

    @fraiseql.input
    class CreateUserInput:
        name: str
        email: str

    @fraiseql.query
    def users(limit: int = 10) -> list[User]:
        return fraiseql.config(sql_source="v_user", returns_list=True)

    # Export minimal types.json (use fraiseql.toml for queries, security, etc.)
    fraiseql.export_types("types.json")
    ```
"""

from fraiseql.async_client import AsyncFraiseQLClient
from fraiseql.client import (
    FraiseQLAuthError,
    FraiseQLClient,
    FraiseQLDatabaseError,
    FraiseQLError,
    FraiseQLRateLimitError,
    FraiseQLUnsupportedError,
)
from fraiseql.decorators import FieldConfig, field, mutation, query, scalar, subscription
from fraiseql.decorators import enum as enum_decorator
from fraiseql.decorators import error as error_decorator
from fraiseql.decorators import input as input_decorator
from fraiseql.decorators import interface as interface_decorator
from fraiseql.decorators import type as type_decorator
from fraiseql.decorators import union as union_decorator
from fraiseql.errors import (
    AuthenticationError,
    GraphQLError,
    NetworkError,
    TimeoutError,
)
from fraiseql.registry import SchemaRegistry, generate_schema_json
from fraiseql.retry import RetryConfig
from fraiseql.scalars import ID, UUID, CustomScalar, Date, DateTime, Decimal, Json, Time, Vector
from fraiseql.schema import config, export_schema, export_types
from fraiseql.scope import ScopeValidationError, describe_scope_format, validate_scope
from fraiseql.validators import (
    ScalarValidationError,
    get_all_custom_scalars,
    validate_custom_scalar,
)

# Aliases for cleaner API (must be defined before __all__ references them)
type = type_decorator
enum = enum_decorator
error = error_decorator
input = input_decorator
interface = interface_decorator
union = union_decorator

__version__ = "2.0.0-alpha.1"

__all__ = [
    # Authoring decorators (clean alias names)
    "type",
    "enum",
    "error",
    "input",
    "interface",
    "union",
    "scalar",
    "query",
    "mutation",
    "subscription",
    "field",
    "FieldConfig",
    # Async client
    "AsyncFraiseQLClient",
    # Client classes (legacy async client)
    "FraiseQLClient",
    "FraiseQLError",
    "FraiseQLAuthError",
    "FraiseQLUnsupportedError",
    "FraiseQLRateLimitError",
    "FraiseQLDatabaseError",
    # Error hierarchy
    "GraphQLError",
    "NetworkError",
    "TimeoutError",
    "AuthenticationError",
    # Retry configuration
    "RetryConfig",
    # Custom scalars
    "CustomScalar",
    # Scalar types
    "ID",
    "UUID",
    "DateTime",
    "Date",
    "Time",
    "Json",
    "Decimal",
    "Vector",
    # Schema utilities
    "config",
    "export_schema",
    "export_types",
    # Schema registry
    "SchemaRegistry",
    "generate_schema_json",
    # Scope validation (RBAC)
    "validate_scope",
    "ScopeValidationError",
    "describe_scope_format",
    # Custom scalar validation
    "validate_custom_scalar",
    "ScalarValidationError",
    "get_all_custom_scalars",
]
