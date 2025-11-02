# Extracted from: docs/performance/caching.md
# Block number: 2
from fastapi import FastAPI, Request

from fraiseql.fastapi import create_fraiseql_app

app = FastAPI()


# Initialize cache at startup
@app.on_event("startup")
async def startup():
    app.state.cache = PostgresCache(pool)
    app.state.result_cache = ResultCache(backend=app.state.cache, default_ttl=300)


# Provide cached repository in GraphQL context
def get_graphql_context(request: Request) -> dict:
    base_repo = FraiseQLRepository(
        pool=app.state.pool,
        context={"tenant_id": request.state.tenant_id, "user_id": request.state.user_id},
    )

    return {
        "request": request,
        "db": CachedRepository(base_repo, app.state.result_cache),
        "tenant_id": request.state.tenant_id,
    }


fraiseql_app = create_fraiseql_app(types=[User, Post, Product], context_getter=get_graphql_context)

app.mount("/graphql", fraiseql_app)
