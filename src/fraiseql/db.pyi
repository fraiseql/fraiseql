from typing import Any, Callable, Optional

from psycopg.sql import SQL, Composed
from psycopg_pool import AsyncConnectionPool

class DatabaseQuery:
    sql: SQL | Composed
    params: tuple[Any, ...]
    row_factory: Callable[[tuple[Any, ...]], Any] | None

    def __init__(
        self,
        sql: SQL | Composed,
        params: tuple[Any, ...],
        row_factory: Callable[[tuple[Any, ...]], Any] | None = None,
    ) -> None: ...

def register_type_for_view(
    view_name: str,
    type_class: type,
    table_columns: Optional[dict[str, type]] = None,
    foreign_keys: Optional[set[str]] = None,
) -> None: ...

class FraiseQLRepository:
    def __init__(
        self, pool: AsyncConnectionPool, context: Optional[dict[str, Any]] = None
    ) -> None: ...
    async def query(
        self,
        view_name: str,
        where: Optional[dict[str, Any]] = None,
        order_by: Optional[list[str] | str] = None,
        limit: Optional[int] = None,
        offset: Optional[int] = None,
        selection_set: Optional[dict[str, Any]] = None,
    ) -> list[dict[str, Any]]: ...
    async def get_by_id(
        self,
        view_name: str,
        id_value: Any,
        selection_set: Optional[dict[str, Any]] = None,
    ) -> Optional[dict[str, Any]]: ...
    async def create(
        self,
        entity_type: str,
        input_data: dict[str, Any],
        selection_set: Optional[dict[str, Any]] = None,
    ) -> dict[str, Any]: ...
    async def update(
        self,
        entity_type: str,
        id_value: Any,
        update_data: dict[str, Any],
        selection_set: Optional[dict[str, Any]] = None,
    ) -> dict[str, Any]: ...
    async def delete(
        self,
        entity_type: str,
        id_value: Any,
        selection_set: Optional[dict[str, Any]] = None,
    ) -> dict[str, Any]: ...
    async def call_function(
        self,
        function_name: str,
        input_data: dict[str, Any],
        selection_set: Optional[dict[str, Any]] = None,
    ) -> dict[str, Any]: ...

async def create_production_pool(
    database: str,
    *,
    host: str = "localhost",
    port: int = 5432,
    user: Optional[str] = None,
    password: Optional[str] = None,
    ssl_mode: str = "prefer",
    **kwargs: Any,
) -> Any: ...
async def create_prototype_pool(
    database: str,
    *,
    host: str = "localhost",
    port: int = 5432,
    user: Optional[str] = None,
    password: Optional[str] = None,
    **kwargs: Any,
) -> Any: ...
async def create_legacy_pool(
    database_url: str,
    **pool_kwargs: Any,
) -> AsyncConnectionPool: ...
