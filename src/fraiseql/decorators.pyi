from collections.abc import Awaitable, Callable
from typing import Any, TypeVar, overload

_F = TypeVar("_F", bound=Callable[..., Any])

@overload
def query(fn: _F) -> _F: ...
@overload
def query() -> Callable[[_F], _F]: ...
def query(fn: _F | None = None) -> _F | Callable[[_F], _F]: ...
@overload
def subscription(fn: _F) -> _F: ...
@overload
def subscription() -> Callable[[_F], _F]: ...
def subscription(fn: _F | None = None) -> _F | Callable[[_F], _F]: ...
@overload
def field(
    *,
    resolver: Callable[..., Awaitable[Any]] | None = None,
    cache_ttl: int | None = None,
) -> Callable[[Callable[..., Any]], Callable[..., Any]]: ...
@overload
def field(
    fn: Callable[..., Any],
    *,
    resolver: Callable[..., Awaitable[Any]] | None = None,
    cache_ttl: int | None = None,
) -> Callable[..., Any]: ...
def field(
    fn: Callable[..., Any] | None = None,
    *,
    resolver: Callable[..., Awaitable[Any]] | None = None,
    cache_ttl: int | None = None,
) -> Any: ...
def turbo_query(
    fn: _F | None = None,
    *,
    cache_ttl: int | None = None,
    max_page_size: int = 100,
) -> _F | Callable[[_F], _F]: ...

class TurboExecutionMarker:
    query_name: str
    view_name: str
    cache_ttl: int | None
    max_page_size: int

    def __init__(
        self,
        query_name: str,
        view_name: str,
        cache_ttl: int | None = None,
        max_page_size: int = 100,
    ) -> None: ...

def connection(
    fn: _F | None = None,
    *,
    view_name: str | None = None,
    max_page_size: int = 100,
) -> _F | Callable[[_F], _F]: ...
