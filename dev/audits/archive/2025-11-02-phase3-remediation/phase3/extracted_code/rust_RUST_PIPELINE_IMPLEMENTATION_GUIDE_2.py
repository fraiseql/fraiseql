# Extracted from: docs/rust/RUST_PIPELINE_IMPLEMENTATION_GUIDE.md
# Block number: 2
from fraiseql import query
from fraiseql.core.rust_pipeline import RustResponseBytes


@query
async def users(info) -> RustResponseBytes:
    """Get all users using Rust pipeline."""
    repo = info.context["repo"]
    return await repo.find_rust("v_user", "users", info)


@query
async def user(info, id: UUID) -> RustResponseBytes:
    """Get single user using Rust pipeline."""
    repo = info.context["repo"]
    return await repo.find_one_rust("v_user", "user", info, id=id)


@query
async def search_users(info, query: str | None = None, limit: int = 20) -> RustResponseBytes:
    """Search users with filtering."""
    repo = info.context["repo"]
    filters = {}
    if query:
        filters["name__icontains"] = query

    return await repo.find_rust("v_user", "users", info, **filters, limit=limit)
