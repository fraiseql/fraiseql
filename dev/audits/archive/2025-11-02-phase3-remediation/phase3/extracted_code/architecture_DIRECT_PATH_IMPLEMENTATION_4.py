# Extracted from: docs/architecture/DIRECT_PATH_IMPLEMENTATION.md
# Block number: 4
try:
    # Direct path...
    return Response(content=bytes(result_bytes), media_type="application/json")
except Exception as e:
    logger.warning(f"Direct path failed, falling back to GraphQL: {e}")
    # Continue to traditional GraphQL execution
