"""The `minimal` conformance fixture, authored with the Python SDK's public API.

Deliberately the smallest useful schema: one type, one query, and **no** enums, inputs,
mutations or subscriptions. That emptiness is the point — `#850` was a producer marshalling
its unpopulated sections to JSON `null`, which the compiler rejects with
`invalid type: null, expected a sequence` and no key name, and it made every shipped Go
example uncompilable. A fixture that populates every section cannot see it.
"""

import fraiseql
from fraiseql.scalars import ID


@fraiseql.type(sql_source="v_user")
class User:
    id: ID
    email: str


@fraiseql.query(sql_source="v_user")
def users() -> list[User]:
    pass
