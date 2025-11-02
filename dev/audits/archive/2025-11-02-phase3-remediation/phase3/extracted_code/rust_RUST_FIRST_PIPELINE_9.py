# Extracted from: docs/rust/RUST_FIRST_PIPELINE.md
# Block number: 9
from starlette.responses import Response

from fraiseql.core.rust_pipeline import RustResponseBytes


def handle_graphql_response(result: Any) -> Response:
    """Handle different response types from FraiseQL resolvers.

    Supports:
    - RustResponseBytes: Pre-serialized bytes from Rust (FASTEST)
    - RawJSONResult: Legacy string-based response
    - dict: Standard GraphQL response (uses Pydantic)
    """
    # 🚀 RUST PIPELINE: Zero-copy bytes → HTTP
    if isinstance(result, RustResponseBytes):
        return Response(
            content=result.bytes,  # Already UTF-8 encoded
            media_type="application/json",
            headers={
                "Content-Length": str(len(result.bytes)),
            },
        )

    # Legacy: String-based response (still bypasses Pydantic)
    if isinstance(result, RawJSONResult):
        return Response(
            content=result.json_string.encode("utf-8"),
            media_type="application/json",
        )

    # Traditional: Pydantic serialization (slowest path)
    return JSONResponse(content=result)
