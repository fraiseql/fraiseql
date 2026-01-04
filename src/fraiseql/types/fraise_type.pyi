from typing import Any, Callable, Optional, TypeVar, dataclass_transform, overload

_T = TypeVar("_T", bound=type[Any])

@dataclass_transform()
@overload
def fraise_type(
    _cls: None = None,
    *,
    sql_source: Optional[str] = None,
    jsonb_column: Optional[str] = ...,
    implements: Optional[list[type]] = None,
    resolve_nested: bool = False,
) -> Callable[[_T], _T]: ...
@overload
def fraise_type(_cls: _T) -> _T: ...
def fraise_type(
    _cls: Optional[_T] = None,
    *,
    sql_source: Optional[str] = None,
    jsonb_column: Optional[str] = ...,
    implements: Optional[list[type]] = None,
    resolve_nested: bool = False,
) -> _T | Callable[[_T], _T]: ...
