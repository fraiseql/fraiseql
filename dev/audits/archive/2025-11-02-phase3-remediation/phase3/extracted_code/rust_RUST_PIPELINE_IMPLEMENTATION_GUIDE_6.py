# Extracted from: docs/rust/RUST_PIPELINE_IMPLEMENTATION_GUIDE.md
# Block number: 6
try:
    result = await repo.find_rust("v_user", "users", info)
    return result  # RustResponseBytes
except Exception as e:
    # Handle database errors, etc.
    logger.error(f"Query failed: {e}")
    # Return appropriate GraphQL error
