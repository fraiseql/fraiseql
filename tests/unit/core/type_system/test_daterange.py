import pytest
from graphql import GraphQLError

from fraiseql.types.scalars.daterange import parse_date_range_value


@pytest.mark.unit
def test_valid_date_range() -> None:
    # Test valid date range formats
    valid_ranges = [
        """[2023-01-01, 2023-01-31]""",
        """(2023-01-01, 2023-01-31)""",
        """[2023-01-01, 2023-01-31)""",
        """(2023-01-01, 2023-01-31]""",
    ]
    for date_range in valid_ranges:
        assert parse_date_range_value(date_range) == date_range


def test_invalid_date_range_format() -> None:
    # Test invalid date range formats
    invalid_ranges = [
        """2023-01-01, 2023-01-31""",
        """[2023-01-01, 2023-01-31""",
        """2023-01-01, 2023-01-31]""",
        """[2023-01-01 2023-01-31]""",
    ]
    for date_range in invalid_ranges:
        with pytest.raises(GraphQLError):
            parse_date_range_value(date_range)


def test_invalid_date_in_range() -> None:
    # Test date ranges with invalid dates
    invalid_date_ranges = [
        """[2023-01-01, 2023-13-32]""",
        """(2023-01-01, 2023-02-30)""",
        """[2023-01-32, 2023-02-28]""",
    ]
    for date_range in invalid_date_ranges:
        with pytest.raises(GraphQLError):
            parse_date_range_value(date_range)


def test_none_value() -> None:
    # Test None value
    assert parse_date_range_value(None) is None


def test_non_string_value() -> None:
    # Test non-string values
    non_string_values = [
        12345,
        {"start": "2023-01-01", "end": "2023-01-31"},
        ["2023-01-01", "2023-01-31"],
    ]
    for value in non_string_values:
        with pytest.raises(GraphQLError):
            parse_date_range_value(value)
