"""Query/mutation signatures under PEP 563 deferred annotations (issue #924).

``from __future__ import annotations`` turns every annotation into a *string* at
definition time. The class path has always resolved those (``_get_class_annotations``
calls ``typing.get_type_hints``); the function path read ``func.__annotations__`` raw
and handed the strings straight to ``python_type_to_graphql``, which mapped none of
them. Every argument came out with the Python source text as its GraphQL type and
``nullable: False`` — inverting optionality on exactly the arguments an author marked
optional, silently, because a bogus type name is only rejected later at compile.

The declarations live at module scope on purpose: ``get_type_hints`` evaluates the
deferred strings against the defining module's globals, so a type declared inside a
test function is unresolvable — and that is also how a real ``schema.py`` is written.
"""

from __future__ import annotations

from fraiseql.scalars import ID  # noqa: TC001 — resolved at runtime by get_type_hints
from fraiseql.types import extract_function_signature


class User:
    """A type referenced by the signatures below, resolvable from module globals."""

    id: ID
    email: str


def get_user(id: ID, tag: str | None) -> User:
    """One required identity argument and one optional filter."""


def list_users(limit: int = 10) -> list[User]:
    """A list return, which PEP 563 also hid behind the string ``"list[User]"``."""


def maybe_user(id: ID) -> User | None:
    """A nullable return."""


def test_optional_argument_stays_optional() -> None:
    """`str | None` is String/nullable — not the source text with nullable False."""
    sig = extract_function_signature(get_user)
    tag = next(a for a in sig["arguments"] if a["name"] == "tag")

    assert tag["type"] == "String"
    assert tag["nullable"] is True


def test_required_argument_keeps_its_scalar_type() -> None:
    """A scalar annotation resolves to its GraphQL scalar, not `"ID"`-as-source-text."""
    sig = extract_function_signature(get_user)
    ident = next(a for a in sig["arguments"] if a["name"] == "id")

    assert ident["type"] == "ID"
    assert ident["nullable"] is False


def test_return_type_resolves() -> None:
    """A registered return type is recognised rather than emitted as a string."""
    sig = extract_function_signature(get_user)

    assert sig["return_type"]["type"] == "User"
    assert sig["return_type"]["nullable"] is False
    assert sig["return_type"]["is_list"] is False


def test_list_return_is_still_a_list() -> None:
    """`list[User]` stays a list; as a string it was neither list nor known type."""
    sig = extract_function_signature(list_users)

    assert sig["return_type"]["type"] == "[User!]"
    assert sig["return_type"]["is_list"] is True


def test_nullable_return_is_still_nullable() -> None:
    """`User | None` keeps its nullability through deferred evaluation."""
    sig = extract_function_signature(maybe_user)

    assert sig["return_type"]["type"] == "User"
    assert sig["return_type"]["nullable"] is True
    assert sig["return_type"]["is_list"] is False


def test_default_values_survive() -> None:
    """Defaults come from `inspect.signature`, and must not regress with the fix."""
    sig = extract_function_signature(list_users)
    limit = next(a for a in sig["arguments"] if a["name"] == "limit")

    assert limit["type"] == "Int"
    assert limit["nullable"] is False
    assert limit["default"] == 10


def test_unresolvable_annotation_falls_back_rather_than_raising() -> None:
    """An unresolvable forward reference degrades to the raw map, as the class path does.

    Mirrors ``_get_class_annotations``: a `NameError` from a name the module cannot see
    must not turn schema authoring into a traceback. The resulting type is wrong — the
    compiler rejects it downstream — but the SDK stays usable for every other symbol.
    """

    def orphan(value: Undeclared) -> User:  # type: ignore[name-defined]  # noqa: F821
        """`Undeclared` exists in no namespace."""

    sig = extract_function_signature(orphan)

    assert sig["arguments"][0]["name"] == "value"
    assert sig["return_type"]["type"] == "User"
