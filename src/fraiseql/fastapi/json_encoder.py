"""Custom JSON encoder for FraiseQL FastAPI responses."""

import datetime
import decimal
import ipaddress
import json
import uuid
from typing import Any


class FraiseQLJSONEncoder(json.JSONEncoder):
    """Custom JSON encoder that handles FraiseQL and PostgreSQL types."""

    def default(self, obj: Any) -> Any:
        """Encode non-standard types to JSON-serializable format."""
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
