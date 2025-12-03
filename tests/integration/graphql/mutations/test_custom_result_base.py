# tests/mutations/test_custom_result_base.py
from dataclasses import is_dataclass
from uuid import UUID, uuid4

import pytest

import fraiseql
from fraiseql.mutations.decorators import success
from fraiseql.types import JSON

pytestmark = pytest.mark.integration


@pytest.mark.unit
@fraiseql.type
class MyResultBase:
    id_: UUID
    status: str
    message: str | None = None
    metadata: JSON | None = None


@success
class CreateDnsServerError(MyResultBase):
    original_payload: JSON | None = None
    conflict_dns_server: str | None = None


def test_success_decorator_supports_base_classes() -> None:
    # FraiseQL types are not dataclasses, they have their own type system
    assert not is_dataclass(CreateDnsServerError)
    assert hasattr(CreateDnsServerError, "__fraiseql_definition__")
    assert hasattr(CreateDnsServerError, "__gql_typename__")

    instance = CreateDnsServerError(
        id_=uuid4(),
        status="error",
        message="Conflict",
        metadata={"reason": "duplicate"},
        original_payload={"ip_address": "1.1.1.1"},
        conflict_dns_server="dns-1",
    )

    assert instance.status == "error"
    assert instance.original_payload is not None
    assert instance.original_payload["ip_address"] == "1.1.1.1"
