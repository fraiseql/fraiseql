"""FraiseQL Core Package.

Exports public API for FraiseQL framework.
"""

from typing import Any

from .fields import fraise_field
from .gql.schema_builder import build_fraiseql_schema
from .mutations.decorators import failure, result, success
from .mutations.mutation_decorator import mutation
from .types import fraise_input, fraise_type
from .types.enum import fraise_enum
from .types.generic import (
    Connection,
    Edge,
    PageInfo,
    PaginatedResponse,
    create_connection,
)
from .types.interface import fraise_interface

# Core aliases
type = fraise_type
input = fraise_input
enum = fraise_enum
interface = fraise_interface


def subscription(x: Any) -> Any:
    return x  # Placeholder for future implementation


# FastAPI integration (optional)
try:
    from .fastapi import FraiseQLConfig, create_fraiseql_app

    _fastapi_available = True
except ImportError:
    _fastapi_available = False
    create_fraiseql_app = None
    FraiseQLConfig = None

# Auth integration (optional)
try:
    from .auth import (
        AuthProvider,
        UserContext,
        requires_auth,
        requires_permission,
        requires_role,
    )
    from .auth.auth0 import Auth0Config, Auth0Provider

    _auth_available = True
except ImportError:
    _auth_available = False
    AuthProvider = None
    UserContext = None
    requires_auth = None
    requires_permission = None
    requires_role = None
    Auth0Config = None
    Auth0Provider = None

# CQRS support
from .cqrs import CQRSExecutor, CQRSRepository

__version__ = "0.1.0"

__all__ = [
    "Auth0Config",
    "Auth0Provider",
    # Auth integration
    "AuthProvider",
    # CQRS support
    "CQRSExecutor",
    "CQRSRepository",
    # Generic types
    "Connection",
    "Edge",
    "FraiseQLConfig",
    "PageInfo",
    "PaginatedResponse",
    "UserContext",
    # Core functionality
    "build_fraiseql_schema",
    "create_connection",
    # FastAPI integration
    "create_fraiseql_app",
    "enum",
    "failure",
    "fraise_enum",
    "fraise_field",
    "fraise_input",
    "fraise_interface",
    "fraise_type",
    "input",
    "interface",
    "mutation",
    "requires_auth",
    "requires_permission",
    "requires_role",
    "result",
    "subscription",
    "success",
    "type",
]
