"""FraiseQL Core Package.

Exports public API for FraiseQL framework.
"""

# Core imports
from .cqrs import CQRSExecutor, CQRSRepository
from .decorators import field, query
from .fields import fraise_field
from .gql.schema_builder import build_fraiseql_schema
from .mutations.decorators import failure, result, success
from .mutations.mutation_decorator import mutation
from .optimization.decorators import dataloader_field
from .subscriptions import subscription
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
from .types.scalars.date import DateField as Date
from .types.scalars.email_address import EmailAddressField as EmailAddress
from .types.scalars.json import JSONField as JSON  # noqa: N814

# Core aliases - renamed to avoid shadowing builtins
fraiseql_type = fraise_type
fraiseql_input = fraise_input
fraiseql_enum = fraise_enum
fraiseql_interface = fraise_interface

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

__version__ = "0.1.0a8"

__all__ = [
    "JSON",
    # Auth integration
    "Auth0Config",
    "Auth0Provider",
    "AuthProvider",
    # CQRS support
    "CQRSExecutor",
    "CQRSRepository",
    # Generic types
    "Connection",
    # Scalar types
    "Date",
    "Edge",
    "EmailAddress",
    "FraiseQLConfig",
    "PageInfo",
    "PaginatedResponse",
    "UserContext",
    # Core functionality
    "build_fraiseql_schema",
    "create_connection",
    # FastAPI integration
    "create_fraiseql_app",
    "dataloader_field",
    "failure",
    "field",
    "fraise_enum",
    "fraise_field",
    "fraise_input",
    "fraise_interface",
    "fraise_type",
    "fraiseql_enum",
    "fraiseql_input",
    "fraiseql_interface",
    "fraiseql_type",
    "mutation",
    "query",
    "requires_auth",
    "requires_permission",
    "requires_role",
    "result",
    "subscription",
    "success",
]
