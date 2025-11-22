"""Simple tests for validation module to improve coverage."""

from dataclasses import dataclass
from typing import Optional

import pytest

import fraiseql
from fraiseql.errors.exceptions import WhereClauseError
from fraiseql.validation import validate_where_input


@pytest.mark.unit
@fraiseql.type
@dataclass
class User:
    """Test user type."""

    id: int
    name: str
    email: str
    age: Optional[int] = None
    is_active: bool = True


@fraiseql.type
@dataclass
class Post:
    """Test post type."""

    id: int
    title: str
    content: str
    author: User
    tags: list[str]


class TestValidateWhereInput:
    """Test the validate_where_input function."""

    def test_valid_simple_where(self) -> None:
        """Test validation of simple where clause."""
        where = {"name": {"_eq": "John"}}
        errors = validate_where_input(where, User)
        assert errors == []

    def test_valid_multiple_fields(self) -> None:
        """Test validation with multiple fields."""
        where = {"name": {"_eq": "John"}, "age": {"_gt": 18}, "email": {"_like": "%@example.com"}}
        errors = validate_where_input(where, User)
        assert errors == []

    def test_invalid_field(self) -> None:
        """Test validation with invalid field."""
        where = {"invalid_field": {"_eq": "value"}}
        errors = validate_where_input(where, User)
        assert len(errors) == 1
        assert "invalid_field" in errors[0]
        assert "Unknown field" in errors[0]

    def test_invalid_operator(self) -> None:
        """Test validation with invalid operator."""
        where = {"name": {"_invalid_op": "value"}}
        errors = validate_where_input(where, User)
        assert len(errors) == 1
        assert "_invalid_op" in errors[0]
        assert "Unknown operator" in errors[0]

    def test_logical_operators_and(self) -> None:
        """Test validation with AND operator."""
        where = {"_and": [{"name": {"_eq": "John"}}, {"age": {"_gt": 18}}]}
        errors = validate_where_input(where, User)
        assert errors == []

    def test_logical_operators_or(self) -> None:
        """Test validation with OR operator."""
        where = {"_or": [{"name": {"_eq": "John"}}, {"email": {"_like": "%gmail.com"}}]}
        errors = validate_where_input(where, User)
        assert errors == []

    def test_logical_operators_not(self) -> None:
        """Test validation with NOT operator."""
        where = {"_not": {"name": {"_eq": "John"}}}
        errors = validate_where_input(where, User)
        assert errors == []

    def test_empty_where(self) -> None:
        """Test validation with empty where clause."""
        errors = validate_where_input({}, User)
        assert errors == []

        errors = validate_where_input(None, User)
        assert errors == []

    def test_non_dict_where(self) -> None:
        """Test validation with non-dict where clause."""
        errors = validate_where_input("not a dict", User)
        assert len(errors) == 1
        assert "must be a dictionary" in errors[0]

    def test_strict_mode_raises_on_error(self) -> None:
        """Test that strict mode raises exception on first error."""
        where = {"invalid_field": {"_eq": "value"}}

        with pytest.raises(WhereClauseError) as exc_info:
            validate_where_input(where, User, strict=True)

        assert "invalid_field" in str(exc_info.value)

    def test_strict_mode_with_invalid_operator(self) -> None:
        """Test strict mode with invalid operator."""
        where = {"name": {"_invalid": "value"}}

        with pytest.raises(WhereClauseError) as exc_info:
            validate_where_input(where, User, strict=True)

        assert "_invalid" in str(exc_info.value)

    def test_multiple_errors_collected(self) -> None:
        """Test that multiple errors are collected in non-strict mode."""
        where = {
            "invalid1": {"_eq": "value"},
            "invalid2": {"_eq": "value"},
            "name": {"_invalid_op": "value"},
        }
        errors = validate_where_input(where, User)
        assert len(errors) >= 3
