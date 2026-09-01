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
        fraiseql.field(description='The user\'s "display" name', deprecated="use displayName"),
    ] = None
    salary: Annotated[float | None, fraiseql.field(requires_scope="read:User.salary")] = None
    # Two words and a digit segment, deliberately (#1249). A one-word name spells the same
    # in every convention, so a fixture made only of them cannot tell an SDK that
    # translates snake_case to camelCase from one that emits the identifier verbatim —
    # uniform in the dimension that selects the branch. `phone_1` is the second, narrower
    # question: the reference collapses a digit segment onto the previous word
    # (`phone_1` → `phone1`, `dns_1_id` → `dns1Id`), and a helper written `/_([a-z])/`
    # silently does not.
    last_login_at: str | None = None
    phone_1: str | None = None


@fraiseql.type(sql_source="v_order")
class Order:
    id: ID
    total: float
    status: str


# `crud=True` is an authoring-time expansion, not a compiled key: the compiler has no
# `crud` concept, so the only evidence an SDK implements it is that the operations and
# input objects it should produce are IN the compiled schema. Nothing asserted that, and
# what the nine generating SDKs produced had drifted three ways — Dart's generator had no
# caller and Ruby's only its own tests (#1241, #1242), Python pointed the mutations at
# `create_ticket` where the other eight wrote `fn_create_ticket` (#1243), and three SDKs
# emitted flat arguments where six emitted an input object (#1246).
#
# `slug` is computed, which is authoring-time too and observable only here: a client
# cannot supply a server-assigned field, so it must be absent from both input objects
# while remaining present on the type. Emitting the flag itself makes the whole document
# uncompilable — `IntermediateField` has no `computed` member and denies unknown fields
# (#927, #1183, #1244).
# The type name is two words on purpose. `Ticket` cannot tell `ticket` from `ticket`, so a
# fixture using it would have passed for the six SDKs that name generated operations in
# snake_case while every hand-authored operation beside them is camelCase (#1247).
# `SupportTicket` makes `supportTicket` and `support_ticket` different strings.
#
# `due_date` is two words for the same reason, and it is here rather than only on `User`
# because the generated input objects are built from the type's fields: Ruby camelCased
# them in `crud_generator` and not in `TypeBuilder`, so one schema emitted `due_date` on
# the type and `dueDate` in `CreateSupportTicketInput` (#1249). Only a two-word field on
# a `crud` type can see that disagreement.
@fraiseql.type(sql_source="v_support_ticket", crud=True)
class SupportTicket:
    id: int
    title: str
    due_date: str
    slug: Annotated[str, fraiseql.field(computed=True)]


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
    # A hand-authored input type's field names are a third registration path, distinct
    # from a type's fields and from the input objects a `crud` type generates (#1249
    # covered those two). `display_name` is two words so the path is exercised at all.
    display_name: str | None = None


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
    # #966's actor allow-list, enforced in the same executor gate as `requires_role` on
    # every transport. It was authorable only by hand-writing `schema.json`, which is the
    # thing every SDK exists to avoid — and a security gate that eleven authoring
    # languages cannot express is a gate nobody can turn on (#1123).
    requires_actor=["human_user", "service_account"],
)
def tenant_orders(include_archived: bool | None = None) -> list[Order]:
    pass


@fraiseql.mutation(
    sql_source="fn_create_user",
    operation="insert",
    invalidates_views=["v_user", "v_user_summary"],
    invalidates_fact_tables=["tf_signup"],
    # #1253: the role gate on the write side. `query_requires_role` has been a construct
    # since this suite was written and the mutation half never was, so eleven mutation
    # builders grew `requires_role` with nothing comparing the result — which is how Go
    # came to have it on queries and not on mutations, found by inspection.
    requires_role="admin",
    # The write side carries it too. `IntermediateMutation::requires_actor` exists and is
    # enforced identically, so a query-only rollout would leave the more consequential
    # half of the gate unauthorable with nothing saying so (#1123).
    requires_actor=["service_account"],
)
def create_user(email: str, name: str | None, display_name: str | None) -> User:
    pass


@fraiseql.mutation(
    sql_source="fn_place_order",
    operation="insert",
    inject={"user_id": "jwt:sub"},
    invalidates_views=["v_order_summary"],
    invalidates_fact_tables=["tf_sale"],
)
def place_order() -> Order:
    pass


@fraiseql.subscription(
    entity_type="Order",
    topic="order_events",
    filter={"conditions": [{"argument": "order_id", "path": "$.id"}]},
    fields=["id", "total"],
)
def order_updated(order_id: ID | None = None) -> Order:
    # No trailing period: the description travels from this docstring, and the canonical
    # fixture's string is what every other SDK passes explicitly.
    """Stream of order update events"""
