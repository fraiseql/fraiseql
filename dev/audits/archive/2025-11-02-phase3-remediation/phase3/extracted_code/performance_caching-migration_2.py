# Extracted from: docs/performance/caching-migration.md
# Block number: 2
def get_graphql_context(request: Request) -> dict:
    repo = FraiseQLRepository(
        pool=app.state.db_pool, context={"tenant_id": request.state.tenant_id}
    )

    return {
        "request": request,
        "db": repo,  # ← Direct repository
        "tenant_id": request.state.tenant_id,
    }
