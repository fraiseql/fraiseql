"""The JSON body both clients send.

It is httpx's own ``json=`` encoding (compact, UTF-8, no NaN), with one addition: a
``decimal.Decimal`` is written as the string of its digits. The server takes a
``Decimal`` argument as a string such as ``"1.50"``, or an exact integer, and refuses a
number with a fraction or an exponent, so a ``Decimal`` must never become a JSON
number, and its string must never use an exponent.
"""

from __future__ import annotations

import json
from decimal import Decimal
from typing import Any

JSON_HEADERS = {"Content-Type": "application/json"}


def _decimal_digits(value: object) -> str:
    if isinstance(value, Decimal):
        if not value.is_finite():
            msg = f"a Decimal variable must be finite, got {value}"
            raise ValueError(msg)
        # Fixed-point: every digit and the scale, never an exponent (1E+3 is "1000").
        return format(value, "f")
    msg = f"Object of type {type(value).__name__} is not JSON serializable"
    raise TypeError(msg)


def encode_body(payload: dict[str, Any]) -> bytes:
    """Encode a GraphQL request body, writing each ``Decimal`` as its exact digits.

    Raises:
        ValueError: A ``Decimal`` is NaN or infinite, or a float is.
        TypeError: A value is neither JSON nor a ``Decimal``.
    """
    return json.dumps(
        payload,
        ensure_ascii=False,
        separators=(",", ":"),
        allow_nan=False,
        default=_decimal_digits,
    ).encode("utf-8")
