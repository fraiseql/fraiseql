from typing import Any, Awaitable, Callable, Optional, TypeVar, overload

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
    resolver: Optional[Callable[..., Awaitable[Any]]] = None,
    cache_ttl: Optional[int] = None,
) -> Callable[[Callable[..., Any]], Callable[..., Any]]: ...
@overload
def field(
    fn: Callable[..., Any],
    *,
    resolver: Optional[Callable[..., Awaitable[Any]]] = None,
    cache_ttl: Optional[int] = None,
) -> Callable[..., Any]: ...
def field(
    fn: Optional[Callable[..., Any]] = None,
    *,
    resolver: Optional[Callable[..., Awaitable[Any]]] = None,
    cache_ttl: Optional[int] = None,
) -> Any: ...
def turbo_query(
    fn: Optional[_F] = None,
    *,
    cache_ttl: Optional[int] = None,
    max_page_size: int = 100,
) -> _F | Callable[[_F], _F]: ...

class TurboExecutionMarker:
    query_name: str
    view_name: str
    cache_ttl: Optional[int]
    max_page_size: int

    def __init__(
        self,
        query_name: str,
        view_name: str,
        cache_ttl: Optional[int] = None,
        max_page_size: int = 100,
    ) -> None: ...

def connection(
    fn: Optional[_F] = None,
    *,
    view_name: Optional[str] = None,
    max_page_size: int = 100,
) -> _F | Callable[[_F], _F]: ...
