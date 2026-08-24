"""The `full` conformance fixture, authored with the Python SDK's public API.

Every declaration here exists to survive `fraiseql compile` and be observable in the
compiled artifact — see `sdks/official/conformance/canonical.md` for the construct-by-
construct rationale and `sdks/official/conformance/project.py` for what is asserted.

The module opens with `from __future__ import annotations` deliberately. It is what
modern Python does, it is what this repository's own `TCH` lint rules push authors
towards, and under it every annotation the decorators see is a *string*. The function
path used to hand those strings straight through, so `name: str | None` exported as a
required argument of a nonexistent type `str | None` (#924) — silent until the compiler
rejected the return type. Removing this line does not weaken a test; it removes the only
place the deferred-annotation path is exercised end to end.
"""

from __future__ import annotations

from enum import Enum
from typing import Annotated

import fraiseql
from fraiseql.scalars import (  # noqa: TC001 — deferred annotations are resolved at runtime
    ID,
    BitVector,
    HalfVector,
    SparseVector,
    Vector,
)


@fraiseql.type(sql_source="v_user", relay=True)
class User:
    id: ID
    email: str
    name: Annotated[
        str | None,
        fraiseql.field(
            description='The user\'s "display" name', deprecated="use displayName"
        ),
    ] = None
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


@fraiseql.type(sql_source="v_document")
class Document:
    id: ID
    embedding: Annotated[
        Vector,
        fraiseql.field(
            vector_config=fraiseql.VectorConfig(
                dimensions=1536, index_type="ivf_flat", distance_metric="l2"
            )
        ),
    ]
    fingerprint: Annotated[
        BitVector,
        fraiseql.field(
            vector_config=fraiseql.VectorConfig(
                dimensions=768, index_type="hnsw", distance_metric="hamming"
            )
        ),
    ]
    compact: Annotated[
        HalfVector | None,
        fraiseql.field(
            vector_config=fraiseql.VectorConfig(
                dimensions=1536, index_type="hnsw", distance_metric="inner_product"
            )
        ),
    ] = None
    terms: Annotated[
        SparseVector | None,
        fraiseql.field(
            vector_config=fraiseql.VectorConfig(
                dimensions=30000, index_type="none", distance_metric="cosine"
            )
        ),
    ] = None
    similarity: Annotated[float, fraiseql.field(vector_distance="embedding")]


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
def user(id: ID) -> User | None:
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


@fraiseql.subscription(
    entity_type="Order",
    topic="order_events",
    filter={"conditions": [{"argument": "orderId", "path": "$.id"}]},
    fields=["id", "total"],
)
def orderUpdated(order_id: ID | None = None) -> Order:  # noqa: N802
    # No trailing period: the description travels from this docstring, and the canonical
    # fixture's string is what every other SDK passes explicitly.
    """Stream of order update events"""
