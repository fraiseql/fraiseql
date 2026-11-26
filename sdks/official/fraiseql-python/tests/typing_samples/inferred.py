"""What a type checker infers for what each generic decorator returns.

The decorators are generic over the class or function they wrap and hand it back, so a
decorated class, or a decorated operation's result, keeps its type. ``assert_type`` fails
the check when an inference changes, e.g. to ``Unknown``.

Each class is decorated by a call on a class that is not itself decorated: asserted against
a decorated name, ``assert_type`` would read that name through the same decorator and pass
whatever it returned. For the same reason the operations return an undecorated class.

Checked by ``tests/test_decorator_typing.py`` with ``ty``; never imported by the suite.
"""

from enum import Enum
from typing import assert_type

import fraiseql
from fraiseql.scalars import ID, CustomScalar


class Plain:
    id: ID


class Choice(Enum):
    OPEN = "open"


class Address(CustomScalar):
    name = "Address"


assert_type(fraiseql.type(Plain), type[Plain])
assert_type(fraiseql.input(Plain), type[Plain])
assert_type(fraiseql.interface(Plain), type[Plain])
assert_type(fraiseql.enum(Choice), type[Choice])
assert_type(fraiseql.scalar(Address), type[Address])
assert_type(fraiseql.error(Plain), type[Plain])
assert_type(fraiseql.error(shareable=True)(Plain), type[Plain])


def plain_event() -> Plain:
    """An event, not decorated."""
    raise NotImplementedError


assert_type(fraiseql.subscription(plain_event)(), Plain)
assert_type(fraiseql.subscription(entity_type="Plain")(plain_event)(), Plain)


@fraiseql.query
def one(id: ID) -> Plain | None:
    """One."""


@fraiseql.mutation
def rename(id: ID, title: str) -> Plain:
    """Rename."""


assert_type(one(ID("1")), Plain | None)
assert_type(rename(ID("1"), "t"), Plain)
