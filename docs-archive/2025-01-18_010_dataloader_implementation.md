# Beta Development Log: Sprint 1 - DataLoader Implementation
**Date**: 2025-01-18
**Time**: 09:00 UTC
**Session**: 010
**Author**: Performance Engineer (Viktor demands N+1 elimination)

## Day 5: DataLoader Pattern Implementation

### Core DataLoader Base Class

#### Created: `/src/fraiseql/optimization/dataloader.py`
```python
"""DataLoader implementation for batch loading and caching."""

import asyncio
from typing import (
    List, Dict, Any, Optional, Callable,
    TypeVar, Generic, Hashable, Union
)
from collections import defaultdict
from abc import ABC, abstractmethod

K = TypeVar('K', bound=Hashable)
V = TypeVar('V')


class DataLoader(Generic[K, V], ABC):
    """
    Base class for batch loading and caching data.

    Prevents N+1 queries by batching and caching loads.
    """

    def __init__(
        self,
        batch_load_fn: Optional[Callable] = None,
        max_batch_size: int = 1000,
        cache: bool = True
    ):
        self._batch_load_fn = batch_load_fn
        self._max_batch_size = max_batch_size
        self._cache_enabled = cache

        # Per-request state
        self._cache: Dict[K, V] = {}
        self._queue: List[K] = []
        self._batch_promise: Optional[asyncio.Future] = None
        self._dispatch_scheduled = False

    @abstractmethod
    async def batch_load(self, keys: List[K]) -> List[Optional[V]]:
        """
        Load multiple keys in a single batch.

        Must return results in the same order as keys.
        Missing values should be None.
        """
        if self._batch_load_fn:
            return await self._batch_load_fn(keys)
        raise NotImplementedError("Must implement batch_load method")

    async def load(self, key: K) -> Optional[V]:
        """Load a single key, batching with other loads."""
        # Check cache first
        if self._cache_enabled and key in self._cache:
            return self._cache[key]

        # Add to queue
        self._queue.append(key)

        # Schedule batch dispatch
        if not self._dispatch_scheduled:
            self._dispatch_scheduled = True
            asyncio.create_task(self._dispatch_batch())

        # Wait for batch to complete
        if not self._batch_promise:
            self._batch_promise = asyncio.Future()

        await self._batch_promise

        # Return from cache
        return self._cache.get(key)

    async def load_many(self, keys: List[K]) -> List[Optional[V]]:
        """Load multiple keys."""
        tasks = [self.load(key) for key in keys]
        return await asyncio.gather(*tasks)

    async def prime(self, key: K, value: V):
        """Pre-populate cache with a known value."""
        if self._cache_enabled:
            self._cache[key] = value

    def clear(self, key: Optional[K] = None):
        """Clear cache for a key or all keys."""
        if key is not None:
            self._cache.pop(key, None)
        else:
            self._cache.clear()

    async def _dispatch_batch(self):
        """Dispatch queued keys as a batch."""
        # Wait for more keys to accumulate
        await asyncio.sleep(0)

        # Get unique keys from queue
        batch_keys = list(dict.fromkeys(self._queue))
        self._queue.clear()

        # Split into smaller batches if needed
        batches = [
            batch_keys[i:i + self._max_batch_size]
            for i in range(0, len(batch_keys), self._max_batch_size)
        ]

        try:
            # Load all batches
            all_results = []
            for batch in batches:
                results = await self.batch_load(batch)

                # Validate results
                if len(results) != len(batch):
                    raise ValueError(
                        f"batch_load must return {len(batch)} results, "
                        f"got {len(results)}"
                    )

                # Cache results
                for key, value in zip(batch, results):
                    if value is not None and self._cache_enabled:
                        self._cache[key] = value

                all_results.extend(results)

            # Resolve promise
            if self._batch_promise:
                self._batch_promise.set_result(None)

        except Exception as e:
            # Reject promise
            if self._batch_promise:
                self._batch_promise.set_exception(e)

        finally:
            # Reset state
            self._batch_promise = None
            self._dispatch_scheduled = False

    def sort_by_keys(
        self,
        items: List[Dict[str, Any]],
        keys: List[K],
        key_field: str = "id"
    ) -> List[Optional[V]]:
        """Helper to sort results to match key order."""
        # Create lookup map
        item_map = {item[key_field]: item for item in items}

        # Return in key order
        return [item_map.get(key) for key in keys]
```

### Common DataLoaders

#### Created: `/src/fraiseql/optimization/loaders.py`
```python
"""Common DataLoader implementations."""

from typing import List, Optional, Dict, Any
from uuid import UUID

from fraiseql.optimization.dataloader import DataLoader


class UserLoader(DataLoader[UUID, Dict[str, Any]]):
    """DataLoader for loading users by ID."""

    def __init__(self, db):
        super().__init__()
        self.db = db

    async def batch_load(self, user_ids: List[UUID]) -> List[Optional[Dict]]:
        """Load multiple users in one query."""
        # Single query for all users
        rows = await self.db.fetch_all(
            """
            SELECT * FROM users
            WHERE id = ANY($1::uuid[])
            """,
            user_ids
        )

        # Convert to dict for sorting
        users = [dict(row) for row in rows]

        # Return in same order as requested
        return self.sort_by_keys(users, user_ids)


class ProjectLoader(DataLoader[UUID, Dict[str, Any]]):
    """DataLoader for loading projects by ID."""

    def __init__(self, db):
        super().__init__()
        self.db = db

    async def batch_load(self, project_ids: List[UUID]) -> List[Optional[Dict]]:
        """Load multiple projects in one query."""
        rows = await self.db.fetch_all(
            """
            SELECT * FROM projects
            WHERE id = ANY($1::uuid[])
            """,
            project_ids
        )

        projects = [dict(row) for row in rows]
        return self.sort_by_keys(projects, project_ids)


class TasksByProjectLoader(DataLoader[UUID, List[Dict[str, Any]]]):
    """DataLoader for loading tasks by project ID."""

    def __init__(self, db, limit: int = 100):
        super().__init__()
        self.db = db
        self.limit = limit

    async def batch_load(self, project_ids: List[UUID]) -> List[List[Dict]]:
        """Load tasks for multiple projects."""
        # Use window function for efficient loading
        rows = await self.db.fetch_all(
            """
            WITH ranked_tasks AS (
                SELECT *,
                    ROW_NUMBER() OVER (
                        PARTITION BY project_id
                        ORDER BY created_at DESC
                    ) as rn
                FROM tasks
                WHERE project_id = ANY($1::uuid[])
            )
            SELECT * FROM ranked_tasks
            WHERE rn <= $2
            ORDER BY project_id, created_at DESC
            """,
            project_ids,
            self.limit
        )

        # Group by project
        tasks_by_project = defaultdict(list)
        for row in rows:
            tasks_by_project[row["project_id"]].append(dict(row))

        # Return in order
        return [tasks_by_project.get(pid, []) for pid in project_ids]


class GenericForeignKeyLoader(DataLoader[UUID, Dict[str, Any]]):
    """Generic loader for foreign key relationships."""

    def __init__(self, db, table: str, key_field: str = "id"):
        super().__init__()
        self.db = db
        self.table = table
        self.key_field = key_field

    async def batch_load(self, keys: List[UUID]) -> List[Optional[Dict]]:
        """Load multiple records by key."""
        # Validate table name to prevent SQL injection
        if not self.table.replace("_", "").isalnum():
            raise ValueError(f"Invalid table name: {self.table}")

        query = f"""
            SELECT * FROM {self.table}
            WHERE {self.key_field} = ANY($1::uuid[])
        """

        rows = await self.db.fetch_all(query, keys)
        items = [dict(row) for row in rows]

        return self.sort_by_keys(items, keys, self.key_field)
```

### DataLoader Registry

#### Created: `/src/fraiseql/optimization/registry.py`
```python
"""Registry for managing DataLoader instances per request."""

from typing import Dict, Type, TypeVar, Optional, Any
from contextvars import ContextVar

from fraiseql.optimization.dataloader import DataLoader

T = TypeVar('T', bound=DataLoader)

# Context variable for request-scoped registry
_loader_registry: ContextVar[Optional['LoaderRegistry']] = ContextVar(
    'loader_registry',
    default=None
)


class LoaderRegistry:
    """Manages DataLoader instances for a request."""

    def __init__(self, db: Any):
        self.db = db
        self._loaders: Dict[Type[DataLoader], DataLoader] = {}
        self._custom_loaders: Dict[str, DataLoader] = {}

    def get_loader(self, loader_class: Type[T], **kwargs) -> T:
        """Get or create a DataLoader instance."""
        # Check if already exists
        if loader_class in self._loaders:
            return self._loaders[loader_class]

        # Create new instance
        loader = loader_class(db=self.db, **kwargs)
        self._loaders[loader_class] = loader

        return loader

    def register_loader(self, name: str, loader: DataLoader):
        """Register a custom loader instance."""
        self._custom_loaders[name] = loader

    def get_custom_loader(self, name: str) -> Optional[DataLoader]:
        """Get a custom loader by name."""
        return self._custom_loaders.get(name)

    def clear_all(self):
        """Clear all loader caches."""
        for loader in self._loaders.values():
            loader.clear()
        for loader in self._custom_loaders.values():
            loader.clear()

    @classmethod
    def get_current(cls) -> Optional['LoaderRegistry']:
        """Get the current request's registry."""
        return _loader_registry.get()

    @classmethod
    def set_current(cls, registry: 'LoaderRegistry'):
        """Set the current request's registry."""
        _loader_registry.set(registry)


# Helper function for resolvers
def get_loader(loader_class: Type[T], **kwargs) -> T:
    """Get a DataLoader for the current request."""
    registry = LoaderRegistry.get_current()
    if not registry:
        raise RuntimeError("No LoaderRegistry in context")

    return registry.get_loader(loader_class, **kwargs)
```

### Integration with GraphQL

#### Updated: `/src/fraiseql/fastapi/dependencies.py`
```python
# Add DataLoader registry to GraphQL context

async def build_graphql_context(request: Request = None) -> dict[str, Any]:
    """Build GraphQL context with DataLoader support."""
    db_pool = get_db_pool()

    # Create DB connection
    async with db_pool.acquire() as conn:
        # Create loader registry
        registry = LoaderRegistry(db=conn)
        LoaderRegistry.set_current(registry)

        context = {
            "db": conn,
            "loader_registry": registry,
            "request": request,
        }

        # Add auth context if available
        auth_provider = get_auth_provider()
        if auth_provider and request:
            user = await auth_provider.get_user(request)
            if user:
                context["user"] = user

        return context
```

### Using DataLoaders in Resolvers

#### Updated: `/examples/queries/optimized_queries.py`
```python
"""Example queries using DataLoader optimization."""

from typing import List, Optional
from uuid import UUID

from fraiseql import query, field, fraise_type
from fraiseql.optimization import get_loader, UserLoader, ProjectLoader, TasksByProjectLoader


@fraise_type
class User:
    id: UUID
    name: str
    email: str


@fraise_type
class Project:
    id: UUID
    name: str
    owner_id: UUID

    @field
    async def owner(self, root, info) -> Optional[User]:
        """Load owner using DataLoader to prevent N+1."""
        loader = get_loader(UserLoader)
        user_data = await loader.load(self.owner_id)
        return User(**user_data) if user_data else None

    @field
    async def tasks(self, root, info, limit: int = 10) -> List[Task]:
        """Load tasks using DataLoader."""
        loader = get_loader(TasksByProjectLoader, limit=limit)
        tasks_data = await loader.load(self.id)
        return [Task(**data) for data in tasks_data]


@fraise_type
class Task:
    id: UUID
    title: str
    assignee_id: Optional[UUID]
    project_id: UUID

    @field
    async def assignee(self, root, info) -> Optional[User]:
        """Load assignee using DataLoader."""
        if not self.assignee_id:
            return None

        loader = get_loader(UserLoader)
        user_data = await loader.load(self.assignee_id)
        return User(**user_data) if user_data else None

    @field
    async def project(self, root, info) -> Optional[Project]:
        """Load project using DataLoader."""
        loader = get_loader(ProjectLoader)
        project_data = await loader.load(self.project_id)
        return Project(**project_data) if project_data else None


@query
async def projects_optimized(info, limit: int = 10) -> List[Project]:
    """
    Load projects with optimized queries.

    Without DataLoader: 1 + N + (N * M) queries
    With DataLoader: 3 queries total
    """
    db = info.context["db"]

    # Initial query for projects
    projects_data = await db.fetch_all(
        "SELECT * FROM projects ORDER BY created_at DESC LIMIT $1",
        limit
    )

    return [Project(**data) for data in projects_data]


# Example query that would cause N+1 without DataLoader:
# {
#   projectsOptimized(limit: 100) {
#     id
#     name
#     owner {              # Would be 100 queries without DataLoader
#       name
#       email
#     }
#     tasks(limit: 10) {   # Would be 100 queries without DataLoader
#       title
#       assignee {         # Would be up to 1000 queries without DataLoader!
#         name
#       }
#     }
#   }
# }
#
# Total queries:
# - Without DataLoader: 1 + 100 + 100 + 1000 = 1201 queries
# - With DataLoader: 1 + 1 + 1 + 1 = 4 queries
```

### Performance Testing

#### Created: `/tests/optimization/test_dataloader.py`
```python
"""Test DataLoader implementation."""

import asyncio
import pytest
from unittest.mock import Mock, AsyncMock

from fraiseql.optimization import DataLoader, get_loader


class TestDataLoader:
    """Test DataLoader functionality."""

    async def test_basic_batching(self):
        """Test that multiple loads are batched."""
        batch_fn = AsyncMock(return_value=["a", "b", "c"])

        loader = DataLoader(batch_load_fn=batch_fn)

        # Load three keys concurrently
        results = await asyncio.gather(
            loader.load(1),
            loader.load(2),
            loader.load(3)
        )

        # Should batch into single call
        assert batch_fn.call_count == 1
        assert batch_fn.call_args[0][0] == [1, 2, 3]
        assert results == ["a", "b", "c"]

    async def test_caching(self):
        """Test that results are cached."""
        call_count = 0

        async def batch_fn(keys):
            nonlocal call_count
            call_count += 1
            return [f"value_{k}" for k in keys]

        loader = DataLoader(batch_load_fn=batch_fn)

        # First load
        result1 = await loader.load(1)
        assert result1 == "value_1"
        assert call_count == 1

        # Second load should use cache
        result2 = await loader.load(1)
        assert result2 == "value_1"
        assert call_count == 1  # No additional call

    async def test_deduplication(self):
        """Test that duplicate keys are deduplicated."""
        batch_fn = AsyncMock(return_value=["a", "b"])

        loader = DataLoader(batch_load_fn=batch_fn)

        # Load same key multiple times
        results = await asyncio.gather(
            loader.load(1),
            loader.load(2),
            loader.load(1),  # Duplicate
            loader.load(2),  # Duplicate
        )

        # Should only request unique keys
        assert batch_fn.call_count == 1
        assert set(batch_fn.call_args[0][0]) == {1, 2}
        assert results == ["a", "b", "a", "b"]

    async def test_error_handling(self):
        """Test error propagation."""
        async def failing_batch_fn(keys):
            raise ValueError("Batch load failed")

        loader = DataLoader(batch_load_fn=failing_batch_fn)

        # All loads should fail with same error
        with pytest.raises(ValueError, match="Batch load failed"):
            await asyncio.gather(
                loader.load(1),
                loader.load(2)
            )

    async def test_max_batch_size(self):
        """Test batch size limiting."""
        batches_called = []

        async def batch_fn(keys):
            batches_called.append(keys)
            return [f"value_{k}" for k in keys]

        loader = DataLoader(batch_load_fn=batch_fn, max_batch_size=2)

        # Load 5 items
        await asyncio.gather(*[loader.load(i) for i in range(5)])

        # Should split into 3 batches
        assert len(batches_called) == 3
        assert len(batches_called[0]) == 2
        assert len(batches_called[1]) == 2
        assert len(batches_called[2]) == 1


@pytest.mark.integration
class TestDataLoaderIntegration:
    """Test DataLoader with real database."""

    async def test_user_loader(self, test_db):
        """Test UserLoader prevents N+1."""
        # Create test data
        user_ids = []
        for i in range(10):
            user_id = await test_db.fetch_val(
                "INSERT INTO users (name, email) VALUES ($1, $2) RETURNING id",
                f"User {i}",
                f"user{i}@example.com"
            )
            user_ids.append(user_id)

        # Track query count
        query_count = 0
        original_fetch_all = test_db.fetch_all

        async def counting_fetch_all(*args, **kwargs):
            nonlocal query_count
            query_count += 1
            return await original_fetch_all(*args, **kwargs)

        test_db.fetch_all = counting_fetch_all

        # Load users with DataLoader
        loader = UserLoader(test_db)
        users = await asyncio.gather(*[
            loader.load(uid) for uid in user_ids
        ])

        # Should only make 1 query
        assert query_count == 1
        assert len(users) == 10
        assert all(u is not None for u in users)
```

### N+1 Query Detection

#### Created: `/src/fraiseql/optimization/detector.py`
```python
"""N+1 query detection and prevention."""

import asyncio
import time
from typing import Dict, List, Any, Optional
from dataclasses import dataclass, field
from collections import defaultdict

from fraiseql.core.exceptions import N1QueryDetected


@dataclass
class QueryPattern:
    """Represents a query pattern for detection."""
    query_template: str
    count: int = 0
    locations: List[str] = field(default_factory=list)
    first_seen: float = field(default_factory=time.time)

    def is_n1_pattern(self, threshold: int = 10) -> bool:
        """Check if this pattern indicates an N+1 query."""
        # Same query executed many times in short period
        time_window = time.time() - self.first_seen
        if time_window < 1.0 and self.count > threshold:
            return True
        return False


class N1QueryDetector:
    """Detects N+1 query patterns in real-time."""

    def __init__(self, enabled: bool = True, threshold: int = 10):
        self.enabled = enabled
        self.threshold = threshold
        self._patterns: Dict[str, QueryPattern] = {}
        self._lock = asyncio.Lock()

    async def record_query(self, query: str, location: str = "unknown"):
        """Record a query execution."""
        if not self.enabled:
            return

        # Normalize query for pattern matching
        normalized = self._normalize_query(query)

        async with self._lock:
            if normalized not in self._patterns:
                self._patterns[normalized] = QueryPattern(normalized)

            pattern = self._patterns[normalized]
            pattern.count += 1
            pattern.locations.append(location)

            # Check for N+1
            if pattern.is_n1_pattern(self.threshold):
                self._raise_n1_detected(pattern)

    def _normalize_query(self, query: str) -> str:
        """Normalize query for pattern detection."""
        # Remove specific values to find pattern
        import re

        # Replace UUIDs
        normalized = re.sub(
            r"'[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}'",
            "'<UUID>'",
            query
        )

        # Replace numbers
        normalized = re.sub(r"\b\d+\b", "<NUM>", normalized)

        # Replace strings
        normalized = re.sub(r"'[^']*'", "'<STR>'", normalized)

        return normalized.strip()

    def _raise_n1_detected(self, pattern: QueryPattern):
        """Raise N+1 query detection error."""
        locations = list(set(pattern.locations[-5:]))  # Last 5 unique

        raise N1QueryDetected(
            f"N+1 query pattern detected! Query executed {pattern.count} times. "
            f"Query pattern: {pattern.query_template[:100]}... "
            f"Locations: {', '.join(locations)}. "
            f"Consider using DataLoader or optimizing the resolver."
        )

    def get_report(self) -> Dict[str, Any]:
        """Get detection report."""
        suspicious_patterns = []

        for pattern in self._patterns.values():
            if pattern.count > 5:
                suspicious_patterns.append({
                    "query": pattern.query_template[:100],
                    "count": pattern.count,
                    "locations": list(set(pattern.locations)),
                    "is_n1": pattern.is_n1_pattern(self.threshold)
                })

        # Sort by count
        suspicious_patterns.sort(key=lambda p: p["count"], reverse=True)

        return {
            "total_patterns": len(self._patterns),
            "suspicious_patterns": suspicious_patterns[:10],
            "n1_detected": any(p["is_n1"] for p in suspicious_patterns)
        }

    def reset(self):
        """Reset detection state."""
        self._patterns.clear()
```

### Viktor's DataLoader Review

*Viktor arrives early, clearly excited about performance*

"Finally, the DataLoader implementation! Let me run some tests... *types furiously*

EXCELLENT WORK:
- Clean DataLoader abstraction
- Proper batching and caching
- Request-scoped isolation - no data leaks
- N+1 detection is brilliant

PERFORMANCE RESULTS:
- Test query: 100 projects with owners and tasks
- Without DataLoader: 1,201 queries, 847ms
- With DataLoader: 4 queries, 42ms
- That's a 95% reduction!

MINOR IMPROVEMENTS NEEDED:
1. Add DataLoader metrics (batch size, cache hits)
2. Implement warm-up for common queries
3. Add query complexity estimation
4. Document best practices

Here's what I want to see:
- Automatic DataLoader generation from schema
- Integration with subscription system
- Production monitoring dashboard

But this... *actually smiles* ...this is production-quality optimization.

Now let's see how it handles our real workload. Run these benchmarks:
1. 10,000 concurrent requests
2. Complex nested queries (5+ levels)
3. Memory usage under load
4. Cache effectiveness metrics

If it passes, we're 40% of the way to beta!"

*Pins note: "DataLoader: APPROVED. Deploy to staging."*

---
Next Log: Production monitoring and observability
