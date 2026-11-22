"""An application module as authors write one: every decorator form, and the uses of what
each decorates that a type checker must accept.

Checked by ``tests/test_decorator_typing.py`` with ``ty``; never imported by the suite.
Annotations are evaluated, as in a module without ``from __future__ import annotations``.
"""

from collections.abc import Callable

import fraiseql
from fraiseql.scalars import ID


@fraiseql.type
class Order:
    id: ID
    title: str


@fraiseql.type(sql_source="v_customer", requires_role="staff")
class Customer:
    id: ID
    name: str


@fraiseql.query
def order(id: ID) -> Order | None:
    """One order."""


@fraiseql.query(sql_source="v_order")
def orders(limit: int = 100) -> list[Order]:
    """List orders."""


@fraiseql.mutation
def rename_order(id: ID, title: str) -> Order:
    """Rename an order."""


@fraiseql.mutation(sql_source="fn_create_customer", operation="CREATE")
def create_customer(name: str) -> Customer:
    """Create a customer."""


def first(order: Order) -> str:
    return order.title


def maybe(customer: Customer | None) -> str | None:
    return customer.name if customer is not None else None


def listed() -> list[Order]:
    return orders(limit=5)


def renamed() -> Order:
    return rename_order(id=ID("1"), title="t")


# Each decorated object is what it decorates, under every form.
order_class: type[Order] = Order
customer_class: type[Customer] = Customer
order_query: Callable[[ID], Order | None] = order
orders_query: Callable[[int], list[Order]] = orders
rename_mutation: Callable[[ID, str], Order] = rename_order
create_mutation: Callable[[str], Customer] = create_customer
