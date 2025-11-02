# Extracted from: docs/rust/RUST_PIPELINE_IMPLEMENTATION_GUIDE.md
# Block number: 15
from fraiseql import query


@query
async def users(info) -> RustResponseBytes:
    try:
        return await repo.find_rust("v_user", "users", info)
    except Exception as e:
        logger.error(f"Failed to fetch users: {e}")
        # Return GraphQL error
        raise GraphQLError("Failed to fetch users")
