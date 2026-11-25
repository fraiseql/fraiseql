"""A ``decimal.Decimal`` variable travels as the exact string of its digits.

The server takes a ``Decimal`` argument as a string of digits (``"1.50"``) or an exact
integer, and refuses a number with a fraction or an exponent. Both clients encode a
``decimal.Decimal`` variable as ``format(d, "f")``: every digit and the scale, never an
exponent. A response's ``Decimal`` is already that string, and ``Decimal(value)`` reads
it exactly.
"""

import json
from decimal import Decimal

import httpx
import pytest

from fraiseql.async_client import AsyncFraiseQLClient
from fraiseql.client import FraiseQLClient

THIRTY_DIGITS = "123456789012345678901234567890"
VALUES = [
    ("1.50", "1.50"),
    ("100.00", "100.00"),
    ("12345678901234567890.12", "12345678901234567890.12"),
    (THIRTY_DIGITS, THIRTY_DIGITS),
    ("-0.00", "-0.00"),
    ("1E+3", "1000"),
    ("1.5E-3", "0.0015"),
]


def _echo(sent: list[bytes]):
    """A server that records the request body and answers with the variables it got."""

    def handler(request: httpx.Request) -> httpx.Response:
        sent.append(request.content)
        variables = json.loads(request.content)["variables"]
        return httpx.Response(200, json={"data": {"echo": variables}})

    return httpx.MockTransport(handler)


async def _sync_client_round_trip(variables: dict) -> tuple[bytes, dict]:
    sent: list[bytes] = []
    client = FraiseQLClient("http://test/graphql")
    client._client = httpx.AsyncClient(transport=_echo(sent))
    try:
        body = await client.execute("query($a: Decimal) { echo(a: $a) }", variables)
    finally:
        await client.close()
    return sent[0], body["data"]["echo"]


async def _async_client_round_trip(variables: dict) -> tuple[bytes, dict]:
    sent: list[bytes] = []
    client = AsyncFraiseQLClient(
        "http://test/graphql", client=httpx.AsyncClient(transport=_echo(sent))
    )
    try:
        data = await client.query("query($a: Decimal) { echo(a: $a) }", variables)
    finally:
        await client.close()
    return sent[0], data["data"]["echo"]


ROUND_TRIPS = [_sync_client_round_trip, _async_client_round_trip]


@pytest.mark.anyio
@pytest.mark.parametrize("round_trip", ROUND_TRIPS)
@pytest.mark.parametrize(("written", "wire"), VALUES)
async def test_a_decimal_variable_is_sent_as_its_exact_digits(round_trip, written, wire):
    body, echoed = await round_trip({"a": Decimal(written)})
    assert json.loads(body)["variables"] == {"a": wire}
    assert f'"a":"{wire}"' in body.decode() or f'"a": "{wire}"' in body.decode()
    assert echoed == {"a": wire}
    assert Decimal(echoed["a"]) == Decimal(written)


@pytest.mark.anyio
@pytest.mark.parametrize("round_trip", ROUND_TRIPS)
async def test_the_scale_survives_the_round_trip(round_trip):
    _, echoed = await round_trip({"a": Decimal("100.00")})
    assert Decimal(echoed["a"]).as_tuple().exponent == -2


@pytest.mark.anyio
@pytest.mark.parametrize("round_trip", ROUND_TRIPS)
async def test_a_decimal_nested_in_an_input_object_is_sent_as_a_string(round_trip):
    body, _ = await round_trip({"a": {"amounts": [Decimal("1.50"), Decimal("2.0")]}})
    assert json.loads(body)["variables"] == {"a": {"amounts": ["1.50", "2.0"]}}


@pytest.mark.anyio
@pytest.mark.parametrize("round_trip", ROUND_TRIPS)
@pytest.mark.parametrize("value", ["NaN", "sNaN", "Infinity", "-Infinity"])
async def test_a_decimal_that_is_not_a_number_is_refused(round_trip, value):
    with pytest.raises(ValueError, match="finite"):
        await round_trip({"a": Decimal(value)})


@pytest.mark.anyio
@pytest.mark.parametrize("round_trip", ROUND_TRIPS)
async def test_a_decimal_response_is_read_exactly(round_trip):
    _, echoed = await round_trip({"a": "12345678901234567890.12"})
    assert echoed["a"] == "12345678901234567890.12"
    assert Decimal(echoed["a"]) == Decimal("12345678901234567890.12")


@pytest.mark.anyio
@pytest.mark.parametrize("round_trip", ROUND_TRIPS)
async def test_an_object_json_cannot_encode_is_still_refused(round_trip):
    with pytest.raises(TypeError, match="not JSON serializable"):
        await round_trip({"a": object()})
