from collections.abc import Coroutine
from typing import Any, Callable, Dict, List, Type

from fastapi import FastAPI
from pydantic import BaseModel

class FraiseQLConfig(BaseModel):
    # Database configuration
    database_url: str
    database_pool_size: int
    database_pool_timeout: int
    database_max_overflow: int

    # Environment and mode
    environment: str
    production_mode: bool

    # GraphQL features
    enable_introspection: bool
    enable_playground: bool
    playground_tool: str
    max_query_depth: int

    # Authentication
    auth_enabled: bool

    # Query complexity
    complexity_enabled: bool
    complexity_max_score: int

    # Performance features
    enable_turbo_router: bool
    turbo_router_cache_size: int
    enable_query_caching: bool
    cache_ttl: int

    # Error handling
    hide_error_details: bool

    # Optional features
    enable_subscriptions: bool
    enable_dataloader: bool
    enable_rate_limiting: bool

    def __init__(
        self,
        *,
        database_url: str,
        environment: str = "development",
        production_mode: bool = False,
        enable_introspection: bool = True,
        enable_playground: bool = True,
        playground_tool: str = "graphiql",
        max_query_depth: int = 10,
        auth_enabled: bool = False,
        complexity_enabled: bool = False,
        complexity_max_score: int = 1000,
        enable_turbo_router: bool = False,
        turbo_router_cache_size: int = 1000,
        enable_query_caching: bool = False,
        cache_ttl: int = 300,
        database_pool_size: int = 10,
        database_pool_timeout: int = 30,
        database_max_overflow: int = 20,
        hide_error_details: bool = False,
        enable_subscriptions: bool = False,
        enable_dataloader: bool = True,
        enable_rate_limiting: bool = False,
        **kwargs: Any,
    ) -> None: ...

def create_fraiseql_app(
    config: FraiseQLConfig | None = None,
    *,
    types: List[Type[Any]] | None = None,
    mutations: List[Type[Any]] | None = None,
    queries: List[Type[Any]] | None = None,
    subscriptions: List[Type[Any]] | None = None,
    context_getter: Callable[..., Coroutine[Any, Any, Dict[str, Any]]] | None = None,
    middleware: List[Any] | None = None,
    cors_origins: List[str] | None = None,
    cors_allow_credentials: bool = True,
    cors_allow_methods: List[str] | None = None,
    cors_allow_headers: List[str] | None = None,
    title: str = "FraiseQL API",
    description: str = "GraphQL API built with FraiseQL",
    version: str = "1.0.0",
    docs_url: str | None = "/docs",
    redoc_url: str | None = "/redoc",
    openapi_url: str | None = "/openapi.json",
    include_in_schema: bool = True,
) -> FastAPI: ...

class TurboRouter:
    def __init__(
        self,
        cache_size: int = 1000,
        enable_complexity_analysis: bool = True,
    ) -> None: ...
    async def execute(
        self,
        query: str,
        variables: Dict[str, Any] | None = None,
        context: Dict[str, Any] | None = None,
        operation_name: str | None = None,
    ) -> Dict[str, Any]: ...

# Development utilities
def get_dev_context() -> Dict[str, Any]: ...

# Configuration helpers
def load_config_from_env() -> FraiseQLConfig: ...
def create_database_pool(database_url: str, **kwargs: Any) -> Any: ...

__all__ = [
    "FraiseQLConfig",
    "TurboRouter",
    "create_database_pool",
    "create_fraiseql_app",
    "get_dev_context",
    "load_config_from_env",
]
