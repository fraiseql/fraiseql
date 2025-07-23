from typing import Any, Callable, Dict, List, Type, TypeVar, overload

from .mutations.error_config import MutationErrorConfig as MutationErrorConfig

_T = TypeVar("_T")
_F = TypeVar("_F", bound=Callable[..., Any])

# Core type decorators
@overload
def type(cls: Type[_T]) -> Type[_T]: ...
@overload
def type(
    *,
    sql_source: str | None = None,
    jsonb_column: str | None = None,
    implements: List[Type[Any]] | None = None,
) -> Callable[[Type[_T]], Type[_T]]: ...
def type(
    cls: Type[_T] | None = None,
    *,
    sql_source: str | None = None,
    jsonb_column: str | None = None,
    implements: List[Type[Any]] | None = None,
) -> Type[_T] | Callable[[Type[_T]], Type[_T]]: ...

@overload
def input(cls: Type[_T]) -> Type[_T]: ...
@overload
def input(
    *,
    description: str | None = None,
) -> Callable[[Type[_T]], Type[_T]]: ...
def input(
    cls: Type[_T] | None = None,
    *,
    description: str | None = None,
) -> Type[_T] | Callable[[Type[_T]], Type[_T]]: ...

def success(cls: Type[_T]) -> Type[_T]: ...
def failure(cls: Type[_T]) -> Type[_T]: ...
def result(cls: Type[_T]) -> Type[_T]: ...
def enum(cls: Type[_T]) -> Type[_T]: ...
def interface(cls: Type[_T]) -> Type[_T]: ...

# Query decorator
@overload
def query(func: _F) -> _F: ...
@overload
def query() -> Callable[[_F], _F]: ...
def query(func: _F | None = None) -> _F | Callable[[_F], _F]: ...

# Field decorator
@overload
def field(func: _F) -> _F: ...
@overload
def field(
    *,
    description: str | None = None,
    deprecation_reason: str | None = None,
) -> Callable[[_F], _F]: ...
def field(
    func: _F | None = None,
    *,
    description: str | None = None,
    deprecation_reason: str | None = None,
) -> _F | Callable[[_F], _F]: ...

# Dataloader field decorator
@overload
def dataloader_field(func: _F) -> _F: ...
@overload
def dataloader_field(
    *,
    loader_key: str | None = None,
    description: str | None = None,
) -> Callable[[_F], _F]: ...
def dataloader_field(
    func: _F | None = None,
    *,
    loader_key: str | None = None,
    description: str | None = None,
) -> _F | Callable[[_F], _F]: ...

# Mutation decorator
def mutation(
    *,
    function: str | None = None,
    schema: str = "graphql",
    context_params: Dict[str, str] | None = None,
    error_config: MutationErrorConfig | None = None,
) -> Callable[[Type[_T]], Type[_T]]: ...

# Subscription decorator
@overload
def subscription(func: _F) -> _F: ...
@overload
def subscription() -> Callable[[_F], _F]: ...
def subscription(func: _F | None = None) -> _F | Callable[[_F], _F]: ...

# Helper function for fields
def fraise_field(
    *,
    description: str | None = None,
    alias: str | None = None,
    deprecation_reason: str | None = None,
    default: Any = ...,
) -> Any: ...

# Scalar field types
class Date:
    def __init__(self, value: str | None = None) -> None: ...

class DateTime:
    def __init__(self, value: str | None = None) -> None: ...

class JSON:
    def __init__(self, value: Any = None) -> None: ...

class EmailAddress:
    def __init__(self, value: str | None = None) -> None: ...

class IpAddress:
    def __init__(self, value: str | None = None) -> None: ...

class MacAddress:
    def __init__(self, value: str | None = None) -> None: ...

class Port:
    def __init__(self, value: int | None = None) -> None: ...

class Hostname:
    def __init__(self, value: str | None = None) -> None: ...

# Generic types
class Connection:
    """GraphQL connection type."""
    edges: List[Any]
    page_info: Any
    total_count: int | None

class Edge:
    """GraphQL edge type."""
    node: Any
    cursor: str

class PageInfo:
    """GraphQL page info type."""
    has_next_page: bool
    has_previous_page: bool
    start_cursor: str | None
    end_cursor: str | None

def create_connection(
    items: List[Any],
    *,
    first: int | None = None,
    after: str | None = None,
    last: int | None = None,
    before: str | None = None,
) -> Connection: ...

# CQRS classes
class CQRSRepository:
    """FraiseQL CQRS Repository."""
    def __init__(self, connection_or_pool: Any) -> None: ...
    async def find(
        self,
        view: str,
        filters: Dict[str, Any] | None = None,
        *,
        limit: int | None = None,
        offset: int | None = None,
        order_by: str | List[str] | None = None,
    ) -> List[Dict[str, Any]]: ...
    async def find_one(
        self,
        view: str,
        filters: Dict[str, Any] | None = None,
    ) -> Dict[str, Any] | None: ...
    async def execute_function(
        self,
        function_name: str,
        params: Dict[str, Any] | None = None,
    ) -> Dict[str, Any]: ...

class CQRSExecutor:
    """FraiseQL CQRS Executor."""
    def __init__(self, repository: CQRSRepository) -> None: ...

# Schema builder
def build_fraiseql_schema(
    types: List[Type[Any]] | None = None,
    mutations: List[Type[Any]] | None = None,
    queries: List[Type[Any]] | None = None,
    subscriptions: List[Type[Any]] | None = None,
) -> Any: ...

# Error configurations
ALWAYS_DATA_CONFIG: MutationErrorConfig
DEFAULT_ERROR_CONFIG: MutationErrorConfig
PRINTOPTIM_ERROR_CONFIG: MutationErrorConfig

# Constants
UNSET: Any

# Auth types (when available)
class AuthProvider:
    """Base authentication provider."""

class UserContext:
    """User context for authenticated requests."""
    user_id: str
    roles: List[str]
    permissions: List[str]

def requires_auth(
    func: _F | None = None,
    *,
    optional: bool = False,
) -> _F | Callable[[_F], _F]: ...

def requires_role(
    role: str,
    *,
    optional: bool = False,
) -> Callable[[_F], _F]: ...

def requires_permission(
    permission: str,
    *,
    optional: bool = False,
) -> Callable[[_F], _F]: ...

class Auth0Config:
    """Auth0 configuration."""
    domain: str
    audience: str
    algorithms: List[str]

class Auth0Provider(AuthProvider):
    """Auth0 authentication provider."""
    def __init__(self, config: Auth0Config) -> None: ...

# FastAPI integration (when available)
try:
    from .fastapi import FraiseQLConfig as FraiseQLConfig
    from .fastapi import create_fraiseql_app as create_fraiseql_app
except ImportError:
    type FraiseQLConfig = None
    type create_fraiseql_app = None

# Aliases for backwards compatibility
fraise_type = type
fraise_input = input
fraise_enum = enum
fraise_interface = interface

__version__: str

__all__ = [
    # Core decorators
    "type",
    "input",
    "success",
    "failure",
    "result",
    "enum",
    "interface",
    "query",
    "field",
    "dataloader_field",
    "mutation",
    "subscription",
    "fraise_field",

    # Scalar types
    "Date",
    "DateTime",
    "JSON",
    "EmailAddress",
    "IpAddress",
    "MacAddress",
    "Port",
    "Hostname",

    # Generic types
    "Connection",
    "Edge",
    "PageInfo",
    "create_connection",

    # CQRS
    "CQRSRepository",
    "CQRSExecutor",

    # Schema
    "build_fraiseql_schema",

    # Error configs
    "MutationErrorConfig",
    "ALWAYS_DATA_CONFIG",
    "DEFAULT_ERROR_CONFIG",
    "PRINTOPTIM_ERROR_CONFIG",

    # Constants
    "UNSET",

    # Auth (optional)
    "AuthProvider",
    "UserContext",
    "requires_auth",
    "requires_role",
    "requires_permission",
    "Auth0Config",
    "Auth0Provider",

    # FastAPI integration (optional)
    "FraiseQLConfig",
    "create_fraiseql_app",

    # Aliases
    "fraise_type",
    "fraise_input",
    "fraise_enum",
    "fraise_interface",
]
