# tests/test_where_generator.py

import uuid
from dataclasses import dataclass
from datetime import UTC, date, datetime

from fraiseql.sql.where_generator import safe_create_where_type


@dataclass
class Person:
    name: str
    age: int
    is_active: bool
    birth_date: date
    created_at: datetime
    uid: uuid.UUID


PersonWhere = safe_create_where_type(Person)


def test_eq_filter() -> None:
    where = PersonWhere(name={"eq": "Alice"})
    sql_str = where.to_sql().as_string(None)
    assert "(data ->> 'name') = 'Alice'" in sql_str


def test_combined_filters() -> None:
    where = PersonWhere(name={"eq": "Alice"}, age={"gt": 21})
    sql_str = where.to_sql().as_string(None)
    assert "(data ->> 'name') = 'Alice'" in sql_str
    assert "(data ->> 'age')::numeric > 21" in sql_str  # Numeric cast applied


def test_in_and_isnull_filters() -> None:
    where = PersonWhere(name={"in": ["Alice", "Bob"]}, is_active={"isnull": False})
    sql_str = where.to_sql().as_string(None)
    assert "(data ->> 'name') IN ('Alice', 'Bob')" in sql_str
    assert "(data ->> 'is_active') IS NOT NULL" in sql_str


def test_uuid_and_date_filters() -> None:
    test_uuid = uuid.UUID("12345678-1234-5678-1234-567812345678")
    test_date = date(2024, 12, 31)
    where = PersonWhere(uid={"eq": test_uuid}, birth_date={"lt": test_date})
    sql_str = where.to_sql().as_string(None)
    # psycopg formats UUIDs without hyphens and adds type casts
    assert "12345678" in sql_str  # UUID is formatted without hyphens
    assert "::uuid" in sql_str  # Type cast is added
    assert "2024-12-31" in sql_str
    assert "::date" in sql_str  # Date type cast


def test_datetime_filter() -> None:
    ts = datetime(2025, 1, 1, 12, 30, 45, tzinfo=UTC)
    where = PersonWhere(created_at={"gte": ts})
    sql_str = where.to_sql().as_string(None)
    # psycopg formats timestamps differently
    assert "2025-01-01" in sql_str
    assert "12:30:45" in sql_str


def test_boolean_values_converted_to_strings() -> None:
    """Test that boolean values are properly cast to boolean type for comparison."""
    where_true = PersonWhere(is_active={"eq": True})
    where_false = PersonWhere(is_active={"eq": False})

    sql_true = where_true.to_sql().as_string(None)
    sql_false = where_false.to_sql().as_string(None)

    # Booleans are now properly cast to boolean type instead of string comparison
    assert "(data ->> 'is_active')::boolean = true" in sql_true
    assert "(data ->> 'is_active')::boolean = false" in sql_false


def test_multiple_operators_same_field() -> None:
    """Test multiple operators on the same field."""
    where = PersonWhere(age={"gte": 18, "lt": 65})
    sql_str = where.to_sql().as_string(None)

    # Should generate proper SQL for range queries with numeric casting
    assert "(data ->> 'age')::numeric >= 18" in sql_str
    assert "(data ->> 'age')::numeric < 65" in sql_str
    assert " AND " in sql_str


def test_empty_filter() -> None:
    """Test that empty filter returns None."""
    where = PersonWhere()
    assert where.to_sql() is None


def test_none_values_ignored() -> None:
    """Test that None values in filters are ignored."""
    where = PersonWhere(name={"eq": "Alice"}, age=None)
    sql_str = where.to_sql().as_string(None)

    assert "Alice" in sql_str
    assert "age" not in sql_str  # age field should not appear
