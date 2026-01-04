"""FraiseQL Core Package.

Exports public API for FraiseQL framework.
"""

# Core imports
from .cqrs import CQRSExecutor, CQRSRepository
from .decorators import connection, field, query
from .fields import fraise_field
from .gql.schema_builder import build_fraiseql_schema
from .mutations.decorators import error, result, success
from .mutations.error_config import (
    ALWAYS_DATA_CONFIG,
    DEFAULT_ERROR_CONFIG,
    STRICT_STATUS_CONFIG,
    MutationErrorConfig,
)
from .mutations.mutation_decorator import mutation
from .optimization.decorators import dataloader_field
from .subscriptions import subscription
from .types import fraise_input, fraise_type
from .types.common import MutationResultBase
from .types.definitions import UNSET
from .types.enum import fraise_enum
from .types.errors import Error
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
from .types.scalars.id_scalar import IDField as ID  # noqa: N814
from .types.scalars.json import JSONField as JSON  # noqa: N814
from .types.scalars.uuid import UUIDField as UUID  # noqa: N814

# Core aliases (internal - not exported to prevent shadowing builtins)
# Use fraiseql.type instead of importing 'type' directly
_type_decorator = fraise_type  # For fraiseql.type access via __getattr__
_input_decorator = fraise_input  # For fraiseql.input access via __getattr__

# These are safe to alias (don't shadow builtins)
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

# Caching integration (optional)
try:
    from .caching import (
        CachedRepository,
        SchemaAnalyzer,
        setup_auto_cascade_rules,
    )

    _caching_available = True
except ImportError:
    _caching_available = False
    CachedRepository = None
    SchemaAnalyzer = None
    setup_auto_cascade_rules = None

# Database utilities (optional)
try:
    from .db import create_db_pool, create_legacy_pool

    _db_available = True
except ImportError:
    _db_available = False
    create_db_pool = None
    create_legacy_pool = None

__version__ = "1.9.1"


# Lazy Rust extension loading for performance optimization
import logging
import os

_logger = logging.getLogger(__name__)
_rust_load_attempted = False
_rust_load_failed = False


def _get_fraiseql_rs():
    """Lazy-load the Rust extension.

    The Rust extension provides 7-10x performance improvement for JSON
    transformation, WHERE clause merging, and other critical operations.

    Returns:
        fraiseql._fraiseql_rs module or None if unavailable.

    Note:
        If loading fails and FRAISEQL_SKIP_RUST is not set, a warning
        is logged to help diagnose performance issues.
    """
    global _rust_load_attempted, _rust_load_failed

    # Allow skipping Rust loading for unit tests via environment variable
    if os.getenv("FRAISEQL_SKIP_RUST") == "1":
        return None

    try:
        import importlib

        rs = importlib.import_module("fraiseql._fraiseql_rs")
        _rust_load_attempted = True
        return rs
    except ImportError as e:
        _rust_load_attempted = True
        _rust_load_failed = True

        # Warn about performance implications
        _logger.warning(
            "Failed to load Rust extension (fraiseql_rs). "
            "Performance will be ~7-10x slower for JSON transformation, "
            "WHERE clause merging, and other critical operations. "
            "Error: %s. "
            "See: https://fraiseql.dev/troubleshooting#rust-loading",
            str(e),
        )
        return None


def __getattr__(name: str):
    """Lazy loading for Rust extension and namespaced decorators.

    This allows fraiseql.type and fraiseql.input to work without them being
    in __all__ (which would encourage dangerous 'from fraiseql import type').
    """
    # Support fraiseql.type and fraiseql.input via attribute access
    if name == "type":
        return _type_decorator
    if name == "input":
        return _input_decorator

    # Lazy load Rust extension
    if name == "fraiseql_rs":
        rs = _get_fraiseql_rs()
        # Cache it for future access
        globals()["fraiseql_rs"] = rs
        return rs
    if name == "_fraiseql_rs":
        rs = _get_fraiseql_rs()
        # Cache it for future access
        globals()["_fraiseql_rs"] = rs
        return rs

    raise AttributeError(f"module '{__name__}' has no attribute '{name}'")


__all__ = [
    "ALWAYS_DATA_CONFIG",
    "DEFAULT_ERROR_CONFIG",
    "ID",
    "JSON",
    "STRICT_STATUS_CONFIG",
    "UNSET",
    "UUID",
    "Auth0Config",
    "Auth0Provider",
    "AuthProvider",
    "CQRSExecutor",
    "CQRSRepository",
    "CachedRepository",
    "Connection",
    "Date",
    "Edge",
    "EmailAddress",
    "Error",
    "FraiseQLConfig",
    "MutationErrorConfig",
    "MutationResultBase",
    "PageInfo",
    "PaginatedResponse",
    "SchemaAnalyzer",
    "UserContext",
    "build_fraiseql_schema",
    "connection",
    "create_connection",
    "create_db_pool",
    "create_fraiseql_app",
    "create_legacy_pool",
    "dataloader_field",
    "enum",
    "error",
    "field",
    "fraise_enum",
    "fraise_field",
    "fraise_input",  # Explicit name - safe to import
    "fraise_interface",
    "fraise_type",  # Explicit name - safe to import
    "fraiseql_rs",
    # "input",  # REMOVED: Shadows builtin - use fraiseql.input or fraise_input
    "interface",
    "mutation",
    "query",
    "requires_auth",
    "requires_permission",
    "requires_role",
    "result",
    "setup_auto_cascade_rules",
    "subscription",
    "success",
    # "type",  # REMOVED: Shadows builtin - use fraiseql.type or fraise_type
]
