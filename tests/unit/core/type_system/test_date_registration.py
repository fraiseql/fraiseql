"""Test Date scalar type registration to prevent duplicates."""

import datetime

import pytest

import fraiseql
from fraiseql.gql.schema_builder import SchemaRegistry


@pytest.fixture(autouse=True)
def clear_registry() -> None:
    """Clear the schema registry before each test."""
    registry = SchemaRegistry.get_instance()
    registry.clear()
    yield
    registry.clear()


def test_date_scalar_single_registration() -> None:
    """Ensure Date scalar is only registered once in the schema."""

    @fraiseql.type
    class Model1:
        name: str
        date1: datetime.date

    @fraiseql.type
    class Model2:
        title: str
        date2: datetime.date

    @fraiseql.query
    async def get_model1(info) -> Model1:
        return Model1(name="Test", date1=datetime.date.today())

    @fraiseql.query
    async def get_model2(info) -> Model2:
        return Model2(title="Test", date2=datetime.date.today())

    # This should not raise a duplicate type error
    registry = SchemaRegistry.get_instance()
    schema = registry.build_schema()

    # Verify Date scalar exists and is unique
    assert "Date" in schema.type_map
    date_type = schema.type_map["Date"]
    assert date_type.name == "Date"

    # Count how many times Date appears in the type map
    date_count = sum(1 for name in schema.type_map if name == "Date")
    assert date_count == 1, f"Date scalar registered {date_count} times, expected 1"


def test_multiple_date_fields_in_single_type() -> None:
    """Test that multiple date fields in one type don't cause issues."""

    @fraiseql.type
    class Event:
        name: str
        start_date: datetime.date
        end_date: datetime.date
        created_date: datetime.date

    @fraiseql.query
    async def get_event(info) -> Event:
        today = datetime.date.today()
        return Event(name="Conference", start_date=today, end_date=today, created_date=today)

    # Should not raise duplicate type error
    registry = SchemaRegistry.get_instance()
    schema = registry.build_schema()

    assert "Date" in schema.type_map
    assert schema.type_map["Date"].name == "Date"


def test_date_in_nested_types() -> None:
    """Test Date scalar in nested type structures."""

    @fraiseql.type
    class Person:
        name: str
        birth_date: datetime.date

    @fraiseql.type
    class Company:
        name: str
        founded_date: datetime.date
        ceo: Person

    @fraiseql.query
    async def get_company(info) -> Company:
        return Company(
            name="TechCorp",
            founded_date=datetime.date(2020, 1, 1),
            ceo=Person(name="Jane Doe", birth_date=datetime.date(1980, 5, 15)),
        )

    # Should not raise duplicate type error
    registry = SchemaRegistry.get_instance()
    schema = registry.build_schema()

    assert "Date" in schema.type_map
    # Verify it's the same Date type instance used everywhere
    company_type = schema.type_map["Company"]
    person_type = schema.type_map["Person"]

    # Both should reference the same Date scalar type
    founded_date_field = company_type.fields["foundedDate"]
    birth_date_field = person_type.fields["birthDate"]

    # Check the field types (might be wrapped in NonNull)
    founded_type = founded_date_field.type
    birth_type = birth_date_field.type

    # Unwrap NonNull if necessary
    if hasattr(founded_type, "of_type"):
        founded_type = founded_type.of_type
    if hasattr(birth_type, "of_type"):
        birth_type = birth_type.of_type

    assert founded_type.name == "Date"
    assert birth_type.name == "Date"
    # Verify they're the same instance
    assert founded_type is birth_type
