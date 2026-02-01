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
    from typing import Annotated

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
        # Protected field - requires scope to access
        salary: Annotated[float, fraiseql.field(requires_scope="read:User.salary")]

    @fraiseql.input
    class CreateUserInput:
        name: str
        email: str

    @fraiseql.query
    def users(limit: int = 10) -> list[User]:
        return fraiseql.config(sql_source="v_user", returns_list=True)

    # Export to JSON
    fraiseql.export_schema("schema.json")
    ```
"""

from fraiseql.analytics import aggregate_query, fact_table
from fraiseql.decorators import FieldConfig, field, mutation, query, subscription
from fraiseql.decorators import enum as enum_decorator
from fraiseql.decorators import input as input_decorator
from fraiseql.decorators import interface as interface_decorator
from fraiseql.decorators import type as type_decorator
from fraiseql.decorators import union as union_decorator
from fraiseql.federation import extends, external, key, provides, requires
from fraiseql.observers import (
    Observer,
    RetryConfig,
    email,
    slack,
    webhook,
)
from fraiseql.observers import (
    observer as observer_decorator,
)
from fraiseql.scalars import ID, UUID, Date, DateTime, Decimal, Json, Time, Vector
from fraiseql.schema import config, export_schema, export_types
from fraiseql.security import (
    AuthorizeConfig,
    AuthzPolicyConfig,
    AuthzPolicyType,
    RoleMatchStrategy,
    RoleRequiredConfig,
    authorize,
    authz_policy,
    role_required,
)

__version__ = "2.0.0-alpha.1"

__all__ = [
    # Decorators
    "type_decorator",
    "enum_decorator",
    "input_decorator",
    "interface_decorator",
    "union_decorator",
    "query",
    "mutation",
    "subscription",
    "field",
    "FieldConfig",
    # Federation
    "key",
    "extends",
    "requires",
    "external",
    "provides",
    # Security
    "authorize",
    "role_required",
    "authz_policy",
    "AuthorizeConfig",
    "RoleRequiredConfig",
    "AuthzPolicyConfig",
    "RoleMatchStrategy",
    "AuthzPolicyType",
    # Observers
    "observer_decorator",
    "Observer",
    "RetryConfig",
    "webhook",
    "slack",
    "email",
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
    # Analytics
    "fact_table",
    "aggregate_query",
    # Metadata
    "__version__",
]

# Aliases for cleaner API
type = type_decorator
enum = enum_decorator
input = input_decorator
interface = interface_decorator
union = union_decorator
observer = observer_decorator
