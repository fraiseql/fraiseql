"""FraiseQL v2 - Python Schema Authoring.

This module provides decorators for defining GraphQL schemas that are compiled
by the FraiseQL Rust engine. NO runtime FFI - decorators output JSON only.

Architecture:
    Python @decorators → schema.json → fraiseql-cli → schema.compiled.json → Rust runtime

Example:
    ```python
    import fraiseql
    from enum import Enum

    @fraiseql.enum
    class OrderStatus(Enum):
        PENDING = "pending"
        SHIPPED = "shipped"

    @fraiseql.type
    class User:
        id: int
        name: str
        email: str

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
from fraiseql.decorators import enum as enum_decorator
from fraiseql.decorators import input as input_decorator
from fraiseql.decorators import interface as interface_decorator
from fraiseql.decorators import mutation, query, subscription
from fraiseql.decorators import type as type_decorator
from fraiseql.decorators import union as union_decorator
from fraiseql.schema import config, export_schema

__version__ = "2.0.0-alpha.1"

__all__ = [
    "type_decorator",
    "enum_decorator",
    "input_decorator",
    "interface_decorator",
    "union_decorator",
    "query",
    "mutation",
    "subscription",
    "config",
    "export_schema",
    "fact_table",
    "aggregate_query",
    "__version__",
]

# Aliases for cleaner API
type = type_decorator
enum = enum_decorator
input = input_decorator
interface = interface_decorator
union = union_decorator
