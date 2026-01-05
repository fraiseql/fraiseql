from collections.abc import AsyncIterator, Coroutine
from contextlib import asynccontextmanager, contextmanager
from typing import Any, Callable, ContextManager, Type

from pydantic import BaseModel

class AxumFraiseQLConfig(BaseModel):
    # Database
    database_url: str
    database_pool_size: int
    database_pool_timeout: int
    database_max_overflow: int

    # Environment
    environment: str
    production_mode: bool

    # GraphQL Features
    enable_introspection: bool
    enable_playground: bool
    playground_tool: str
    max_query_depth: int

    # Security
    auth_enabled: bool
    jwt_secret: str | None
    jwt_algorithm: str

    # Performance
    enable_query_caching: bool
    cache_ttl: int

    # Error handling
    hide_error_details: bool

    # Axum HTTP Server
    axum_host: str
    axum_port: int
    axum_workers: int | None
    axum_metrics_token: str

    # CORS
    cors_origins: list[str] | None
    cors_allow_credentials: bool
    cors_allow_methods: list[str] | None
    cors_allow_headers: list[str] | None

    # Compression
    enable_compression: bool
    compression_algorithm: str
    compression_min_bytes: int

    def __init__(
        self,
        *,
        database_url: str,
        database_pool_size: int = 10,
        database_pool_timeout: int = 30,
        database_max_overflow: int = 20,
        environment: str = "development",
        production_mode: bool = False,
        enable_introspection: bool = True,
        enable_playground: bool = True,
        playground_tool: str = "graphiql",
        max_query_depth: int = 10,
        auth_enabled: bool = False,
        jwt_secret: str | None = None,
        jwt_algorithm: str = "HS256",
        enable_query_caching: bool = False,
        cache_ttl: int = 300,
        hide_error_details: bool = False,
        axum_host: str = "127.0.0.1",
        axum_port: int = 8000,
        axum_workers: int | None = None,
        axum_metrics_token: str = "",
        cors_origins: list[str] | None = None,
        cors_allow_credentials: bool = True,
        cors_allow_methods: list[str] | None = None,
        cors_allow_headers: list[str] | None = None,
        enable_compression: bool = True,
        compression_algorithm: str = "brotli",
        compression_min_bytes: int = 256,
        **kwargs: Any,
    ) -> None: ...
    @property
    def effective_workers(self) -> int: ...
    @property
    def server_url(self) -> str: ...
    @classmethod
    def from_env(cls) -> AxumFraiseQLConfig: ...
    def to_dict(self) -> dict[str, Any]: ...

class AxumServer:
    def __init__(self, config: AxumFraiseQLConfig) -> None: ...

    # Type Registration
    def register_types(self, types: list[Type[Any]]) -> None: ...
    def register_mutations(self, mutations: list[Type[Any]]) -> None: ...
    def register_queries(self, queries: list[Type[Any]]) -> None: ...
    def register_subscriptions(self, subscriptions: list[Type[Any]]) -> None: ...
    def add_middleware(self, middleware: Any) -> None: ...

    # Lifecycle Management
    def start(
        self,
        host: str | None = None,
        port: int | None = None,
        workers: int | None = None,
    ) -> None: ...
    async def start_async(
        self,
        host: str | None = None,
        port: int | None = None,
    ) -> None: ...
    async def shutdown(self) -> None: ...
    def is_running(self) -> bool: ...

    # Request Execution
    def execute_query(
        self,
        query: str,
        variables: dict[str, Any] | None = None,
        operation_name: str | None = None,
        context: dict[str, Any] | None = None,
    ) -> dict[str, Any]: ...
    async def execute_query_async(
        self,
        query: str,
        variables: dict[str, Any] | None = None,
        operation_name: str | None = None,
    ) -> dict[str, Any]: ...

    # Configuration & Introspection
    def get_config(self) -> AxumFraiseQLConfig: ...
    def get_schema(self) -> dict[str, Any]: ...
    def get_metrics(self) -> str: ...

    # Type Introspection
    def registered_types(self) -> list[str]: ...
    def registered_mutations(self) -> list[str]: ...
    def registered_queries(self) -> list[str]: ...
    def registered_subscriptions(self) -> list[str]: ...

    # Context Managers
    @contextmanager
    def running(self, host: str = "127.0.0.1", port: int = 8000) -> ContextManager[AxumServer]: ...
    @asynccontextmanager
    async def running_async(
        self, host: str = "127.0.0.1", port: int = 8000
    ) -> AsyncIterator[AxumServer]: ...

def create_axum_fraiseql_app(
    *,
    config: AxumFraiseQLConfig | None = None,
    database_url: str | None = None,
    types: list[Type[Any]] | None = None,
    mutations: list[Type[Any]] | None = None,
    queries: list[Type[Any]] | None = None,
    subscriptions: list[Type[Any]] | None = None,
    context_getter: Callable[..., Coroutine[Any, Any, dict[str, Any]]] | None = None,
    middleware: list[Any] | None = None,
    cors_origins: list[str] | None = None,
    cors_allow_credentials: bool = True,
    cors_allow_methods: list[str] | None = None,
    cors_allow_headers: list[str] | None = None,
    title: str = "FraiseQL API",
    description: str = "GraphQL API built with FraiseQL",
    version: str = "1.0.0",
    docs_url: str | None = "/docs",
    redoc_url: str | None = "/redoc",
    openapi_url: str | None = "/openapi.json",
    include_in_schema: bool = True,
    **kwargs: Any,
) -> AxumServer: ...
def create_production_app(
    *,
    database_url: str,
    types: list[Type[Any]] | None = None,
    mutations: list[Type[Any]] | None = None,
    **kwargs: Any,
) -> AxumServer: ...

__all__ = [
    "AxumFraiseQLConfig",
    "AxumServer",
    "create_axum_fraiseql_app",
    "create_production_app",
]
