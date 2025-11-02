# Extracted from: docs/performance/caching-migration.md
# Block number: 3
from fraiseql.caching import CachedRepository


def get_graphql_context(request: Request) -> dict:
    base_repo = FraiseQLRepository(
        pool=app.state.db_pool,
        context={"tenant_id": request.state.tenant_id},  # REQUIRED!
    )

    # Wrap with caching
    cached_repo = CachedRepository(base_repository=base_repo, cache=app.state.result_cache)

    return {
        "request": request,
        "db": cached_repo,  # ← Cached repository
        "tenant_id": request.state.tenant_id,
    }
