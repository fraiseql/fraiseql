"""Custom response handler for raw JSON passthrough.

This module provides a custom response class and middleware to detect
RawJSONResult objects and return them directly without JSON serialization.
"""

from typing import Any

from fastapi import Response
from starlette.background import BackgroundTask

from fraiseql.core.raw_json_executor import RawJSONResult


class RawJSONResponse(Response):
    """A response class that returns raw JSON strings directly."""

    def __init__(
        self,
        content: str,
        status_code: int = 200,
        headers: dict[str, str] | None = None,
        media_type: str = "application/json",
        background: BackgroundTask | None = None,
    ) -> None:
        """Initialize a raw JSON response.

        Args:
            content: The raw JSON string
            status_code: HTTP status code
            headers: Optional HTTP headers
            media_type: The media type (default: application/json)
            background: Optional background task
        """
        super().__init__(
            content=content,
            status_code=status_code,
            headers=headers,
            media_type=media_type,
            background=background,
        )


def create_json_response(data: Any) -> Response | Any:
    """Create appropriate response based on data type.

    If data is a RawJSONResult, returns a RawJSONResponse that bypasses
    JSON serialization. Otherwise returns the data for normal serialization.

    Args:
        data: The response data

    Returns:
        Either a RawJSONResponse or the original data
    """
    if isinstance(data, RawJSONResult):
        return RawJSONResponse(
            content=data.json_string,
            media_type=data.content_type,
        )

    return data


async def raw_json_middleware(request, call_next):
    """Middleware to handle RawJSONResult in responses.

    This middleware checks if the response contains a RawJSONResult
    and converts it to a proper raw response.

    Args:
        request: The incoming request
        call_next: The next middleware/handler

    Returns:
        The response
    """
    response = await call_next(request)

    # Check if response has a body that's a RawJSONResult
    if hasattr(response, "body") and isinstance(response.body, RawJSONResult):
        return RawJSONResponse(
            content=response.body.json_string,
            status_code=response.status_code,
            headers=dict(response.headers),
            media_type=response.body.content_type,
        )

    return response
