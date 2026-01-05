from collections.abc import Callable
from typing import Any, TypeVar, dataclass_transform, overload

_T = TypeVar("_T", bound=type[Any])

@dataclass_transform()
@overload
def fraise_type(
    _cls: None = None,
    *,
    sql_source: str | None = None,
    jsonb_column: str | None = ...,
    implements: list[type] | None = None,
    resolve_nested: bool = False,
) -> Callable[[_T], _T]: ...
@overload
def fraise_type(_cls: _T) -> _T: ...
def fraise_type(
    _cls: _T | None = None,
    *,
    sql_source: str | None = None,
    jsonb_column: str | None = ...,
    implements: list[type] | None = None,
    resolve_nested: bool = False,
) -> _T | Callable[[_T], _T]: ...
