"""Response handlers for different FraiseQL response types.

This module provides FastAPI response handling for various FraiseQL result types,
including the new Rust-first pipeline responses.
"""

from typing import Any

from starlette.responses import Response

from fraiseql.core.rust_pipeline import RustResponseBytes


def handle_graphql_response(result: Any) -> Response:
    """Handle different response types from FraiseQL resolvers.

    Supports:
    - RustResponseBytes: Pre-serialized bytes from Rust (FASTEST)
    - dict: Standard GraphQL response (uses Pydantic)

    Args:
        result: The result from a FraiseQL resolver

    Returns:
        FastAPI Response object
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

    # Traditional: Pydantic serialization (slowest path)
    from fastapi import JSONResponse

    return JSONResponse(content=result)
