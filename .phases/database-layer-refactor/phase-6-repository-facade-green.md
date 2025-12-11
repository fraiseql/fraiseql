# Phase 6: Repository Facade

**Phase:** GREEN (Make Tests Pass)
**Duration:** 4-6 hours
**Risk:** Medium-High

---

## Objective

**TDD Phase GREEN:** Create thin facade maintaining public API, wire all components together.

Create:
- New FraiseQLRepository that delegates to extracted components
- Maintains 100% API compatibility
- Dependency injection for all components

---

## Files to Create

1. `src/fraiseql/db/repository.py` - Repository facade (~200 lines)
2. `src/fraiseql/db/__init__.py` - Public API exports

---

## Implementation

FraiseQLRepository becomes a thin orchestrator:

```python
class FraiseQLRepository:
    def __init__(self, pool, context=None):
        self._connection_manager = ConnectionManager(pool, context)
        self._type_registry = get_default_registry()
        self._query_builder = QueryBuilderFactory()
        self._where_builder = WhereClauseBuilder()

    async def find(self, view_name, **kwargs):
        # Delegate to components
        query, params = self._query_builder.build_find(view_name, **kwargs)
        results = await self._connection_manager.execute(query, params)
        return self._process_results(results, view_name)
```

---

## Next Phase

→ **Phase 7:** Refactor & Optimize
