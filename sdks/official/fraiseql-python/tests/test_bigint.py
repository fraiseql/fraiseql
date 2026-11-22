"""``fraiseql.BigInt``: a signed 64-bit integer, served as the string of its digits.

A Python ``int`` has no width, and is the type for small values, so it stays ``Int``; a
field whose values pass 32 bits says so with the marker. ``Int`` refuses such a value at
read time.
"""

import fraiseql
from fraiseql.registry import SchemaRegistry
from fraiseql.scalars import ID, BigInt
from fraiseql.types import python_type_to_graphql


def test_the_marker_is_bigint_and_int_stays_int() -> None:
    assert python_type_to_graphql(BigInt) == ("BigInt", False)
    assert python_type_to_graphql(BigInt | None) == ("BigInt", True)
    assert python_type_to_graphql(list[BigInt]) == ("[BigInt!]", False)
    assert python_type_to_graphql(int) == ("Int", False)
    assert fraiseql.BigInt is BigInt


def test_a_bigint_field_and_argument_are_the_built_in_scalar() -> None:
    """Exported as ``BigInt``, and never declared as a custom scalar of that name."""
    SchemaRegistry.clear()

    @fraiseql.type(sql_source="v_ledger")
    class Ledger:
        id: ID
        amount: BigInt
        balance: BigInt | None
        transfer_ids: list[BigInt]

    @fraiseql.query(sql_source="v_ledger")
    def ledgers(account_number: BigInt) -> list[Ledger]:
        pass

    schema = SchemaRegistry.get_schema()
    fields = {f["name"]: f for f in schema["types"][0]["fields"]}
    assert fields["amount"]["type"] == "BigInt"
    assert fields["balance"]["type"] == "BigInt"
    assert fields["balance"]["nullable"] is True
    assert fields["transferIds"]["type"] == "[BigInt!]"
    assert schema["queries"][0]["arguments"][0]["type"] == "BigInt"
    assert "BigInt" not in [s["name"] for s in schema.get("custom_scalars", [])]
    SchemaRegistry.clear()
