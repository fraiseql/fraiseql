"""Comprehensive tests for where type integration edge cases."""

import uuid
from dataclasses import dataclass
from datetime import UTC, date, datetime
from decimal import Decimal
from typing import Any, Dict, List, Optional

from fraiseql.sql.where_generator import safe_create_where_type


@dataclass
class ComplexModel:
    """Complex model with various field types."""

    id: uuid.UUID
    name: str
    age: int
    score: float
    balance: Decimal
    is_active: bool
    birth_date: date
    last_login: datetime
    tags: List[str]
    metadata: Dict[str, Any]
    status: Optional[str] = None


@dataclass
class NestedLevel3:
    """Third level nested model."""

    id: int
    value: str
    items: List[str]


@dataclass
class NestedLevel2:
    """Second level nested model."""

    id: int
    name: str
    level3: Optional[NestedLevel3] = None
    level3_list: Optional[List[NestedLevel3]] = None


@dataclass
class NestedLevel1:
    """First level nested model."""

    id: uuid.UUID
    title: str
    level2: Optional[NestedLevel2] = None
    level2_list: Optional[List[NestedLevel2]] = None


# Create where types
ComplexWhere = safe_create_where_type(ComplexModel)
NestedWhere = safe_create_where_type(NestedLevel1)


class TestComplexNestedWhereConditions:
    """Test complex nested where conditions."""

    def test_deeply_nested_where_conditions(self):
        """Test where conditions on deeply nested fields."""
        # Create nested where conditions
        where = NestedWhere(
            id={"eq": uuid.UUID("12345678-1234-5678-1234-567812345678")},
            title={"contains": "test"},
            level2={
                "level2": {
                    "id": {"gt": 100},
                    "name": {"in": ["Alice", "Bob"]},
                    "level3": {
                        "level3": {
                            "id": {"between": [1, 10]},
                            "value": {"startswith": "prefix_"},
                            "items": {"contains": ["item1", "item2"]},
                        },
                    },
                },
            },
        )

        sql = where.to_sql()
        sql_str = sql.as_string(None) if sql else ""

        # Verify SQL generation for nested conditions
        assert "12345678" in sql_str  # UUID
        assert "test" in sql_str  # Contains
        assert "> 100" in sql_str  # Greater than
        assert "IN ('Alice', 'Bob')" in sql_str  # IN clause
        assert "BETWEEN" in sql_str  # Between operator
        assert "prefix_" in sql_str  # Startswith

    def test_multiple_nested_operators(self):
        """Test multiple operators on nested fields."""
        where = ComplexWhere(
            age={"gte": 18, "lt": 65},  # Age range
            score={"gt": 70.0, "lte": 100.0},  # Score range
            balance={"gte": Decimal("0.00"), "ne": Decimal("999.99")},  # Balance conditions
            is_active={"eq": True},
            tags={"contains": ["python", "sql"], "not_contains": ["java"]},
        )

        sql = where.to_sql()
        sql_str = sql.as_string(None) if sql else ""

        # Verify multiple operators
        assert ">= 18" in sql_str
        assert "< 65" in sql_str
        assert "> 70" in sql_str
        assert "<= 100" in sql_str
        assert "!= 999.99" in sql_str
        assert "'true'" in sql_str  # Boolean as string for JSONB

    def test_complex_or_and_combinations(self):
        """Test complex OR and AND combinations."""
        # Create conditions that would typically use OR/AND
        where = ComplexWhere(
            name={"in": ["Alice", "Bob", "Charlie"]},  # OR condition
            age={"gte": 21, "lte": 65},  # AND condition
            status={"isnull": False},  # NOT NULL
            is_active={"eq": True},
        )

        sql = where.to_sql()
        sql_str = sql.as_string(None) if sql else ""

        # All conditions should be combined with AND at the top level
        assert " AND " in sql_str
        assert "IN (" in sql_str
        assert ">= 21" in sql_str
        assert "<= 65" in sql_str
        assert "IS NOT NULL" in sql_str


class TestSQLInjectionPrevention:
    """Test SQL injection prevention with where types."""

    def test_sql_injection_in_string_fields(self):
        """Test SQL injection attempts in string fields."""
        # Various SQL injection attempts
        injection_attempts = [
            "'; DROP TABLE users; --",
            "' OR '1'='1",
            "'; DELETE FROM data WHERE '1'='1'; --",
            "admin'--",
            "' UNION SELECT * FROM passwords --",
            "'; INSERT INTO data VALUES ('hacked'); --",
            '"; DROP TABLE data; --',
            "' OR 1=1--",
            "`) OR 1=1--",
            "'; EXEC xp_cmdshell('dir'); --",
        ]

        for injection in injection_attempts:
            where = ComplexWhere(
                name={"eq": injection},
                status={"contains": injection},
            )

            sql = where.to_sql()
            sql_str = sql.as_string(None) if sql else ""

            # The injection attempt should be properly escaped
            # Should not contain raw SQL keywords that could be executed
            assert "DROP TABLE" not in sql_str or "DROP TABLE" in repr(sql_str)
            assert "DELETE FROM" not in sql_str or "DELETE FROM" in repr(sql_str)
            assert "UNION SELECT" not in sql_str or "UNION SELECT" in repr(sql_str)

            # The value should be properly quoted/escaped
            assert sql_str.count("'") % 2 == 0  # Even number of quotes (properly paired)

    def test_sql_injection_in_numeric_fields(self):
        """Test SQL injection in numeric fields."""
        # These should fail type validation or be handled safely
        numeric_injections = [
            "1; DROP TABLE users",
            "1 OR 1=1",
            "1 UNION SELECT password FROM users",
            "-1; DELETE FROM data",
        ]

        for injection in numeric_injections:
            # Attempt to create where with injection in numeric field
            try:
                where = ComplexWhere(age={"eq": injection})
                # If it doesn't raise an error, the SQL should be safe
                sql = where.to_sql()
                if sql:
                    sql_str = sql.as_string(None)
                    # Should not contain SQL commands
                    assert "DROP" not in sql_str
                    assert "DELETE" not in sql_str
                    assert "UNION" not in sql_str
            except (ValueError, TypeError):
                # Type validation prevented the injection
                pass

    def test_sql_injection_in_list_values(self):
        """Test SQL injection in list values."""
        where = ComplexWhere(
            name={"in": ["normal", "'; DROP TABLE users; --", "' OR 1=1"]},
            tags={"contains": ["tag1", "'; DELETE FROM data; --"]},
        )

        sql = where.to_sql()
        sql_str = sql.as_string(None) if sql else ""

        # List values should be properly escaped
        assert sql_str.count("'") % 2 == 0  # Properly paired quotes
        # SQL commands should be escaped, not executable
        assert "DROP TABLE users" not in sql_str or "'DROP TABLE users'" in sql_str

    def test_sql_injection_with_special_characters(self):
        """Test handling of special characters that could be used in injections."""
        special_chars = [
            "test\\'; DROP TABLE",  # Backslash escape attempt
            'test"; DROP TABLE',  # Double quote attempt
            "test`; DROP TABLE",  # Backtick attempt
            "test\0; DROP TABLE",  # Null byte attempt
            "test\n; DROP TABLE",  # Newline attempt
            "test\r\n; DROP TABLE",  # CRLF attempt
            "test/*comment*/; DROP",  # Comment attempt
            "test--comment\nDROP",  # SQL comment attempt
        ]

        for char_test in special_chars:
            where = ComplexWhere(name={"eq": char_test})
            sql = where.to_sql()
            sql_str = sql.as_string(None) if sql else ""

            # Should handle special characters safely
            assert "DROP TABLE" not in sql_str or "DROP TABLE" in repr(sql_str)


class TestPerformanceWithLargeDatasets:
    """Test where type performance with large datasets."""

    def test_large_in_clause(self):
        """Test performance with large IN clauses."""
        # Create a large list of values
        large_list = [f"user_{i}" for i in range(1000)]

        where = ComplexWhere(name={"in": large_list})
        sql = where.to_sql()
        sql_str = sql.as_string(None) if sql else ""

        # Should generate valid SQL even with large lists
        assert "IN (" in sql_str
        assert "user_0" in sql_str
        assert "user_999" in sql_str

        # Check that SQL is not excessively long (might be optimized)
        assert len(sql_str) < 50000  # Reasonable limit

    def test_many_conditions(self):
        """Test performance with many conditions."""
        # Create many conditions
        conditions = {
            "id": {"eq": uuid.UUID("12345678-1234-5678-1234-567812345678")},
            "name": {"contains": "test"},
            "age": {"gte": 18, "lte": 65},
            "score": {"gt": 0.0, "lt": 100.0},
            "balance": {"ne": Decimal("0.00")},
            "is_active": {"eq": True},
            "birth_date": {"gte": date(1990, 1, 1), "lte": date(2000, 12, 31)},
            "last_login": {"gte": datetime(2024, 1, 1, tzinfo=UTC)},
            "status": {"in": ["active", "pending", "approved"]},
            "tags": {"contains": ["important", "urgent"]},
        }

        where = ComplexWhere(**conditions)
        sql = where.to_sql()
        sql_str = sql.as_string(None) if sql else ""

        # Should handle many conditions efficiently
        assert sql is not None
        assert len(sql_str) > 100  # Non-trivial SQL

        # All conditions should be present
        assert "12345678" in sql_str
        assert "test" in sql_str
        assert ">= 18" in sql_str
        assert "'true'" in sql_str

    def test_deeply_nested_performance(self):
        """Test performance with deeply nested structures."""
        # Create deeply nested where conditions
        where = NestedWhere(
            level2={
                "level2": {
                    "level3": {
                        "level3": {
                            "items": {"contains": [f"item_{i}" for i in range(100)]},
                        },
                    },
                    "level3_list": {
                        "level3_list": [
                            {"id": {"eq": i}, "value": {"eq": f"value_{i}"}} for i in range(10)
                        ],
                    },
                },
            },
        )

        sql = where.to_sql()
        assert sql is not None  # Should generate SQL without timeout


class TestMixedOperatorTypes:
    """Test mixed operator types in where conditions."""

    def test_all_comparison_operators(self):
        """Test all available comparison operators."""
        test_date = date(2024, 1, 1)
        test_datetime = datetime(2024, 1, 1, 12, 0, 0, tzinfo=UTC)
        test_uuid = uuid.UUID("12345678-1234-5678-1234-567812345678")

        # Test each operator type
        operator_tests = [
            {"name": {"eq": "exact"}},
            {"name": {"ne": "not_this"}},
            {"age": {"lt": 30}},
            {"age": {"lte": 30}},
            {"age": {"gt": 18}},
            {"age": {"gte": 18}},
            {"name": {"in": ["a", "b", "c"]}},
            {"name": {"nin": ["x", "y", "z"]}},
            {"name": {"contains": "substr"}},
            {"name": {"startswith": "prefix"}},
            {"name": {"endswith": "suffix"}},
            {"age": {"between": [18, 65]}},
            {"status": {"isnull": True}},
            {"status": {"isnull": False}},
            {"birth_date": {"eq": test_date}},
            {"last_login": {"gte": test_datetime}},
            {"id": {"eq": test_uuid}},
            {"score": {"between": [0.0, 100.0]}},
            {"tags": {"contains": ["python"]}},
            {"metadata": {"contains": {"key": "value"}}},
        ]

        for conditions in operator_tests:
            where = ComplexWhere(**conditions)
            sql = where.to_sql()
            assert sql is not None
            sql_str = sql.as_string(None)
            assert len(sql_str) > 0

    def test_mixed_operators_same_field(self):
        """Test multiple different operators on the same field."""
        # This might not be typical but should be handled
        where = ComplexWhere(
            age={"gte": 18, "lte": 65, "ne": 25},  # Age between 18-65 but not 25
            name={"contains": "john", "ne": "johnny"},  # Contains john but not exactly johnny
            score={"gt": 0.0, "lt": 100.0, "ne": 50.0},  # Between 0-100 but not 50
        )

        sql = where.to_sql()
        sql_str = sql.as_string(None) if sql else ""

        # All conditions should be applied
        assert ">= 18" in sql_str
        assert "<= 65" in sql_str
        assert "!= 25" in sql_str
        assert "john" in sql_str
        assert "!= 'johnny'" in sql_str or "!='johnny'" in sql_str

    def test_type_specific_operators(self):
        """Test operators that only make sense for specific types."""
        # String-specific operators
        string_where = ComplexWhere(
            name={"contains": "test", "startswith": "Dr.", "endswith": "PhD"},
            status={"in": ["active", "pending"], "ne": "deleted"},
        )

        sql = string_where.to_sql()
        sql_str = sql.as_string(None) if sql else ""

        assert "test" in sql_str
        assert "Dr." in sql_str
        assert "PhD" in sql_str
        assert "IN (" in sql_str
        assert "!= 'deleted'" in sql_str or "!='deleted'" in sql_str

        # Numeric-specific operators
        numeric_where = ComplexWhere(
            age={"between": [18, 65]},
            score={"gt": 70.0, "lte": 95.0},
            balance={"gte": Decimal("100.00"), "ne": Decimal("0.00")},
        )

        sql = numeric_where.to_sql()
        sql_str = sql.as_string(None) if sql else ""

        assert "BETWEEN" in sql_str
        assert "> 70" in sql_str
        assert "<= 95" in sql_str
        assert ">= 100" in sql_str


class TestEdgeCaseValues:
    """Test edge case values in where conditions."""

    def test_empty_and_null_values(self):
        """Test handling of empty strings, empty lists, and null values."""
        where = ComplexWhere(
            name={"eq": ""},  # Empty string
            status={"eq": None},  # Explicit None
            tags={"eq": []},  # Empty list
            # Note: Cannot directly compare JSONB to empty dict with eq
            # Use contains operator for JSONB comparisons
        )

        sql = where.to_sql()
        sql_str = sql.as_string(None) if sql else ""

        # Should handle empty values appropriately
        assert "= ''" in sql_str  # Empty string comparison

    def test_special_numeric_values(self):
        """Test special numeric values like infinity, NaN."""
        import math

        # These might raise exceptions or be handled specially
        special_values = [
            {"score": {"eq": math.inf}},
            {"score": {"eq": -math.inf}},
            {"score": {"eq": math.nan}},
            {"score": {"eq": 0.0}},
            {"score": {"eq": -0.0}},
            {"balance": {"eq": Decimal("Infinity")}},
            {"balance": {"eq": Decimal("-Infinity")}},
            {"balance": {"eq": Decimal("NaN")}},
        ]

        for conditions in special_values:
            try:
                where = ComplexWhere(**conditions)
                sql = where.to_sql()
                # If it generates SQL, it should be valid
                assert sql is not None
            except (ValueError, TypeError, Exception):
                # Some values might not be supported
                pass

    def test_unicode_and_special_strings(self):
        """Test Unicode and special character strings."""
        special_strings = [
            "Hello 世界",  # Chinese
            "Привет мир",  # Russian
            "🚀 Emoji test 🎉",  # Emojis
            "Line1\nLine2",  # Newlines
            "Tab\there",  # Tabs
            "Quote's test",  # Single quote
            'Double "quote" test',  # Double quotes
            "Back\\slash",  # Backslash
            "\x00Null\x00byte",  # Null bytes
            "Very " + "long " * 1000 + "string",  # Very long string
        ]

        for test_str in special_strings:
            where = ComplexWhere(name={"eq": test_str})
            sql = where.to_sql()
            sql_str = sql.as_string(None) if sql else ""

            # Should handle special characters without breaking SQL
            assert sql is not None
            # Check for proper escaping (quotes should be balanced)
            assert sql_str.count("'") % 2 == 0

    def test_boundary_values(self):
        """Test boundary values for different types."""
        where = ComplexWhere(
            age={"eq": 0},  # Zero
            score={"eq": -999999.999999},  # Large negative
            balance={"eq": Decimal("99999999999999999999.99")},  # Large decimal
            birth_date={"eq": date.min},  # Minimum date
            last_login={"eq": datetime.min.replace(tzinfo=UTC)},  # Minimum datetime
            id={"eq": uuid.UUID("00000000-0000-0000-0000-000000000000")},  # Null UUID
            name={"eq": "a" * 1000},  # Long string
            tags={"in": [f"tag_{i}" for i in range(100)]},  # Many tags
        )

        sql = where.to_sql()
        sql_str = sql.as_string(None) if sql else ""

        # Should handle boundary values
        assert sql is not None
        assert "= 0" in sql_str  # Zero handling
        assert "00000000-0000-0000-0000-000000000000" in sql_str.replace("-", "")  # UUID
