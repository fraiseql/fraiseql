"""Custom JSON encoder for FraiseQL FastAPI responses."""

import datetime
import decimal
import ipaddress
import json
import uuid
from typing import Any

from fastapi.responses import JSONResponse

from fraiseql.types.definitions import UNSET


def clean_unset_values(obj: Any) -> Any:
    """Recursively clean UNSET values from an object, converting them to None.

    This is useful for cleaning data structures before they get serialized
    by standard JSON encoders that don't handle UNSET values.

    Args:
        obj: The object to clean (dict, list, or primitive)

    Returns:
        A copy of the object with all UNSET values converted to None
    """
    if obj is UNSET:
        return None
    if isinstance(obj, dict):
        return {key: clean_unset_values(value) for key, value in obj.items()}
    if isinstance(obj, list):
        return [clean_unset_values(item) for item in obj]
    return obj


class FraiseQLJSONEncoder(json.JSONEncoder):
    """Custom JSON encoder that handles FraiseQL and PostgreSQL types."""

    def default(self, obj: Any) -> Any:
        """Encode non-standard types to JSON-serializable format."""
        # Handle UNSET (convert to None for JSON serialization)
        if obj is UNSET:
            return None

        # Handle date and datetime
        if isinstance(obj, datetime.datetime):
            return obj.isoformat()
        if isinstance(obj, datetime.date):
            return obj.isoformat()
        if isinstance(obj, datetime.time):
            return obj.isoformat()

        # Handle UUID
        if isinstance(obj, uuid.UUID):
            return str(obj)

        # Handle Decimal
        if isinstance(obj, decimal.Decimal):
            return float(obj)

        # Handle IP addresses
        if isinstance(obj, (ipaddress.IPv4Address, ipaddress.IPv6Address)):
            return str(obj)

        # Handle bytes
        if isinstance(obj, bytes):
            return obj.decode("utf-8", errors="replace")

        # Fall back to default
        return super().default(obj)


class FraiseQLJSONResponse(JSONResponse):
    """Custom JSON response that uses FraiseQLJSONEncoder."""

    def render(self, content: Any) -> bytes:
        """Render content using FraiseQLJSONEncoder."""
        return json.dumps(
            content,
            cls=FraiseQLJSONEncoder,
            ensure_ascii=False,
            allow_nan=False,
            indent=None,
            separators=(",", ":"),
        ).encode("utf-8")
