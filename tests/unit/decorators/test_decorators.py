import uuid
from uuid import UUID

import pytest

from fraiseql.fields import FRAISE_MISSING, fraise_field
from fraiseql.mutations.decorators import error, success
from fraiseql.types import JSON
from fraiseql.utils.fraiseql_builder import collect_fraise_fields


@pytest.mark.unit
@success
class BaseResult:
    id_: UUID
    status: str
    message: str | None = None
    metadata: JSON | None = None
    updated_fields: list[str] = fraise_field(default_factory=list)
    errors: list[str] = fraise_field(default_factory=list)


@success
class DummySuccess(BaseResult):
    user: str
    count: int = 0
    tags: list[str] = fraise_field(default_factory=list)
    data: dict = fraise_field(default_factory=dict, purpose="output")


@error
class DummyFailure(BaseResult):
    error_code: str
    details: str = "No specific details provided."
    reasons: list[str] = fraise_field(default_factory=list)


def test_success_decorator_correctly_sets_fields() -> None:
    instance = DummySuccess(
        id_=uuid.uuid4(),
        updated_fields=["user"],
        status="success",
        message="User created",
        metadata={"source": "test"},
        errors=[],
        user="test_user",
        count=1,
        tags=["new", "user"],
        data={"key": "value"},
    )
    assert instance.user == "test_user"
    assert instance.count == 1
    assert instance.tags == ["new", "user"]
    assert instance.data == {"key": "value"}


def test_success_decorator_handles_default_factory_correctly() -> None:
    field_map, _ = collect_fraise_fields(DummySuccess)

    assert field_map["tags"].default is FRAISE_MISSING
    assert callable(field_map["tags"].default_factory)
    assert field_map["tags"].default_factory() == []

    assert field_map["data"].default is FRAISE_MISSING
    assert callable(field_map["data"].default_factory)
    assert field_map["data"].default_factory() == {}

    assert field_map["count"].default == 0
    assert field_map["count"].default_factory is None

    assert field_map["user"].default is FRAISE_MISSING
    assert field_map["user"].default_factory is None


def test_success_decorator_field_order() -> None:
    field_map, _ = collect_fraise_fields(DummySuccess)
    names = [field.name for field in field_map.values()]
    assert names == [
        "id_",
        "status",
        "message",
        "metadata",
        "updated_fields",  # User-defined in BaseResult
        "errors",  # User-defined in BaseResult (not auto-injected)
        "id",  # Auto-injected when entity field present (v1.8.1)
        "user",
        "count",
        "tags",
        "data",
    ]


def test_success_constructor_accepts_all_fields() -> None:
    DummySuccess(
        id_=uuid.uuid4(),
        status="ok",
        updated_fields=[],
        errors=[],
        user="alice",
        count=0,
        tags=[],
        data={},
    )


def test_failure_decorator_field_order() -> None:
    instance = DummyFailure(
        id_=uuid.uuid4(),
        updated_fields=[],
        status="failed",
        message="Operation failed",
        metadata={},
        errors=["Invalid input"],
        error_code="INVALID_INPUT",
        details="Input validation failed.",
        reasons=["Input data malformed"],
    )
    assert instance.error_code == "INVALID_INPUT"
    assert instance.details == "Input validation failed."
    assert instance.reasons == ["Input data malformed"]
    field_map, _ = collect_fraise_fields(DummyFailure)
    names = [field.name for field in field_map.values()]
    assert names == [
        "id_",
        "status",
        "message",
        "metadata",
        "updated_fields",  # User-defined in BaseResult (preserved)
        "errors",  # User-defined in BaseResult (preserved)
        "id",  # Auto-injected (v1.8.1)
        "error_code",
        "details",
        "reasons",
        "code",  # Auto-injected in v1.8.1
    ]


def test_failure_decorator_handles_default_factory_correctly() -> None:
    field_map, _ = collect_fraise_fields(DummyFailure)

    assert field_map["reasons"].default is FRAISE_MISSING
    assert callable(field_map["reasons"].default_factory)
    assert field_map["reasons"].default_factory() == []

    assert field_map["details"].default == "No specific details provided."
    assert field_map["details"].default_factory is None

    assert field_map["error_code"].default is FRAISE_MISSING
    assert field_map["error_code"].default_factory is None
