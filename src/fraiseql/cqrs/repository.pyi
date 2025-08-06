from collections.abc import AsyncGenerator
from typing import Any, Dict, List

class CQRSRepository:
    def __init__(self, connection_or_pool: Any) -> None: ...
    async def find(
        self,
        view: str,
        filters: Dict[str, Any] | None = None,
        *,
        limit: int | None = None,
        offset: int | None = None,
        order_by: str | List[str] | None = None,
        select_fields: List[str] | None = None,
        distinct: bool = False,
    ) -> List[Dict[str, Any]]: ...
    async def find_one(
        self,
        view: str,
        filters: Dict[str, Any] | None = None,
        *,
        select_fields: List[str] | None = None,
    ) -> Dict[str, Any] | None: ...
    async def count(
        self,
        view: str,
        filters: Dict[str, Any] | None = None,
    ) -> int: ...
    async def execute_function(
        self,
        function_name: str,
        params: Dict[str, Any] | None = None,
        *,
        schema: str | None = None,
    ) -> Dict[str, Any]: ...
    async def execute_function_with_context(
        self,
        function_name: str,
        context_args: List[Any],
        params: Dict[str, Any] | None = None,
        *,
        schema: str | None = None,
    ) -> Dict[str, Any]: ...
    async def stream(
        self,
        view: str,
        filters: Dict[str, Any] | None = None,
        *,
        batch_size: int = 1000,
        order_by: str | List[str] | None = None,
    ) -> AsyncGenerator[Dict[str, Any]]: ...
    async def exists(
        self,
        view: str,
        filters: Dict[str, Any] | None = None,
    ) -> bool: ...
    async def begin_transaction(self) -> None: ...
    async def commit_transaction(self) -> None: ...
    async def rollback_transaction(self) -> None: ...
    async def close(self) -> None: ...

class CQRSExecutor:
    def __init__(
        self,
        repository: CQRSRepository,
        *,
        enable_caching: bool = False,
        cache_ttl: int = 300,
        enable_query_logging: bool = False,
    ) -> None: ...
    async def execute_query(
        self,
        query_name: str,
        params: Dict[str, Any] | None = None,
        *,
        cache_key: str | None = None,
    ) -> Any: ...
    async def execute_mutation(
        self,
        mutation_name: str,
        params: Dict[str, Any] | None = None,
        *,
        context: Dict[str, Any] | None = None,
    ) -> Any: ...

# Pagination helpers
class PaginationResult:
    items: List[Dict[str, Any]]
    total_count: int
    has_next_page: bool
    has_previous_page: bool
    start_cursor: str | None
    end_cursor: str | None

async def paginate(
    repository: CQRSRepository,
    view: str,
    *,
    filters: Dict[str, Any] | None = None,
    first: int | None = None,
    after: str | None = None,
    last: int | None = None,
    before: str | None = None,
    order_by: str | List[str] | None = None,
) -> PaginationResult: ...

__all__ = [
    "CQRSExecutor",
    "CQRSRepository",
    "PaginationResult",
    "paginate",
]
