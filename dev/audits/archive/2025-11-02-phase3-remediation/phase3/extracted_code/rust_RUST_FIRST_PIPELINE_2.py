# Extracted from: docs/rust/RUST_FIRST_PIPELINE.md
# Block number: 2
from fraiseql import query


@query
async def users(info) -> RustResponseBytes:
    repo = info.context["repo"]
    return await repo.find_rust("v_user", "users", info)
