"""A decorated class used wrongly: the checker must still report it.

Checked by ``tests/test_decorator_typing.py`` with ``ty``, which must find the error below;
never imported by the suite.
"""

from __future__ import annotations

import fraiseql


@fraiseql.type
class Order:
    title: str


def first(order: Order) -> int:
    return order.title
