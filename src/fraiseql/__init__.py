"""FraiseQL Core Package.

Exports public API for FraiseQL framework.
"""

# Core imports
from .cqrs import CQRSExecutor, CQRSRepository
from .decorators import field, query
from .fields import fraise_field
from .gql.schema_builder import build_fraiseql_schema
from .mutations.decorators import failure, result, success
from .mutations.error_config import (
    ALWAYS_DATA_CONFIG,
    DEFAULT_ERROR_CONFIG,
    PRINTOPTIM_ERROR_CONFIG,
    MutationErrorConfig,
)
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

# Core aliases
type = fraise_type  # noqa: A001
input = fraise_input  # noqa: A001
enum = fraise_enum
interface = fraise_interface

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

__version__ = "0.1.0b15"

__all__ = [
    "ALWAYS_DATA_CONFIG",
    "DEFAULT_ERROR_CONFIG",
    "JSON",
    "PRINTOPTIM_ERROR_CONFIG",
    "Auth0Config",
    "Auth0Provider",
    "AuthProvider",
    "CQRSExecutor",
    "CQRSRepository",
    "Connection",
    "Date",
    "Edge",
    "EmailAddress",
    "FraiseQLConfig",
    "MutationErrorConfig",
    "PageInfo",
    "PaginatedResponse",
    "UserContext",
    "build_fraiseql_schema",
    "create_connection",
    "create_fraiseql_app",
    "dataloader_field",
    "enum",
    "failure",
    "field",
    "fraise_enum",
    "fraise_field",
    "fraise_input",
    "fraise_interface",
    "fraise_type",
    "input",
    "interface",
    "mutation",
    "query",
    "requires_auth",
    "requires_permission",
    "requires_role",
    "result",
    "subscription",
    "success",
    "type",
]
