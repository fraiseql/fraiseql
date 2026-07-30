"""The `full` conformance fixture, authored with the Python SDK's public API.

Every declaration here exists to survive `fraiseql compile` and be observable in the
compiled artifact — see `sdks/official/conformance/canonical.md` for the construct-by-
construct rationale and `sdks/official/conformance/project.py` for what is asserted.
"""

from enum import Enum
from typing import Annotated

import fraiseql
from fraiseql.scalars import ID


@fraiseql.type(sql_source="v_user", relay=True)
class User:
    id: ID
    email: str
    name: Annotated[str | None, fraiseql.field(description='The user\'s "display" name')] = None
    salary: Annotated[float | None, fraiseql.field(requires_scope="read:User.salary")] = None


@fraiseql.type(sql_source="v_order")
class Order:
    id: ID
    total: float
    status: str


@fraiseql.error
class UserNotFound:
    message: str
    code: str


@fraiseql.enum
class OrderStatus(Enum):
    PENDING = "PENDING"
    SHIPPED = "SHIPPED"
    CANCELLED = "CANCELLED"


@fraiseql.input
class CreateUserInput:
    email: str
    name: str | None = None


@fraiseql.query(sql_source="v_user")
def users() -> list[User]:
    pass


@fraiseql.query(sql_source="v_user")
def user(id: ID) -> User | None:  # noqa: A002 — `id` is the canonical argument name
    pass


@fraiseql.query(
    sql_source="v_order",
    inject={"tenant_id": "jwt:tenant_id"},
    cache_ttl_seconds=300,
    requires_role="admin",
)
def tenantOrders() -> list[Order]:  # noqa: N802 — camelCase GraphQL field name
    pass


@fraiseql.mutation(
    sql_source="fn_create_user",
    operation="insert",
    invalidates_views=["v_user", "v_user_summary"],
    invalidates_fact_tables=["tf_signup"],
)
def createUser(email: str, name: str | None) -> User:  # noqa: N802
    pass


@fraiseql.mutation(
    sql_source="fn_place_order",
    operation="insert",
    inject={"user_id": "jwt:sub"},
    invalidates_views=["v_order_summary"],
    invalidates_fact_tables=["tf_sale"],
)
def placeOrder() -> Order:  # noqa: N802
    pass
