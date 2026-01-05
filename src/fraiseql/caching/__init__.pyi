from dataclasses import dataclass
from typing import Any

class CacheKeyBuilder:
    def __init__(self) -> None: ...
    def build_key(self, query: str, params: dict[str, Any]) -> str: ...

@dataclass
class CacheConfig:
    enabled: bool
    backend: str
    ttl: int | None
    max_size: int | None

@dataclass
class CacheStats:
    hits: int
    misses: int
    evictions: int
    size: int

class PostgresCacheError(Exception): ...

class PostgresCache:
    async def get(self, key: str) -> Any | None: ...
    async def set(self, key: str, value: Any, ttl: int | None = None) -> None: ...
    async def delete(self, key: str) -> None: ...
    async def clear(self) -> None: ...
    async def get_stats(self) -> CacheStats: ...

class CacheBackend:
    async def get(self, key: str) -> Any | None: ...
    async def set(self, key: str, value: Any, ttl: int | None = None) -> None: ...
    async def delete(self, key: str) -> None: ...
    async def clear(self) -> None: ...

class ResultCache:
    def __init__(self, backend: CacheBackend, config: CacheConfig) -> None: ...
    async def get(self, key: str) -> Any | None: ...
    async def set(self, key: str, value: Any, ttl: int | None = None) -> None: ...

async def cached_query(
    key: str,
    query_fn: Any,
    cache: ResultCache,
    ttl: int | None = None,
) -> Any: ...
@dataclass
class CascadeRule:
    source_table: str
    target_table: str
    foreign_key: str
    action: str

class SchemaAnalyzer:
    async def analyze(self) -> list[CascadeRule]: ...
    def get_cascade_rules(self) -> list[CascadeRule]: ...

async def setup_auto_cascade_rules(schema: Any) -> None: ...

class CachedRepository:
    def __init__(
        self,
        repository: Any,
        cache: CacheBackend,
        config: CacheConfig | None = None,
    ) -> None: ...
    async def query(
        self,
        view_name: str,
        where: dict[str, Any] | None = None,
        order_by: list[str] | str | None = None,
        limit: int | None = None,
        offset: int | None = None,
        selection_set: dict[str, Any] | None = None,
    ) -> list[dict[str, Any]]: ...
    async def create(
        self,
        entity_type: str,
        input_data: dict[str, Any],
        selection_set: dict[str, Any] | None = None,
    ) -> dict[str, Any]: ...
    async def update(
        self,
        entity_type: str,
        id_value: Any,
        update_data: dict[str, Any],
        selection_set: dict[str, Any] | None = None,
    ) -> dict[str, Any]: ...
    async def delete(
        self,
        entity_type: str,
        id_value: Any,
        selection_set: dict[str, Any] | None = None,
    ) -> dict[str, Any]: ...
    async def get_stats(self) -> CacheStats: ...
