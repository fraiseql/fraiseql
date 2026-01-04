from dataclasses import dataclass
from typing import Any

@dataclass
class GraphQLContext:
    db: FraiseQLRepository
    user: UserContext | None = None
    request: Any | None = None
    response: Any | None = None
    loader_registry: LoaderRegistry | None = None
    config: FraiseQLConfig | None = None
    authenticated: bool = False
    _extras: dict[str, Any] = ...

    @classmethod
    def from_dict(cls, context_dict: dict[str, Any]) -> GraphQLContext: ...
    def to_dict(self) -> dict[str, Any]: ...
    def get_extra(self, key: str, default: Any = None) -> Any: ...
    def set_extra(self, key: str, value: Any) -> None: ...

def build_context(
    db: FraiseQLRepository,
    *,
    user: UserContext | None = None,
    request: Any | None = None,
    response: Any | None = None,
    loader_registry: LoaderRegistry | None = None,
    config: FraiseQLConfig | None = None,
    authenticated: bool | None = None,
    **extras: Any,
) -> GraphQLContext: ...

# Forward references for circular imports
from fraiseql.auth.base import UserContext
from fraiseql.cqrs.repository import FraiseQLRepository
from fraiseql.fastapi.config import FraiseQLConfig
from fraiseql.utils.dataloader import LoaderRegistry
