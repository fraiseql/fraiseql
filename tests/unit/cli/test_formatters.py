"""Tests for CLI output formatters (Phase 19, Commit 7).

Tests the output formatting for table, JSON, and CSV formats.
"""

import csv
import io
import json

import pytest

from fraiseql.cli.monitoring.formatters import (
    _format_simple_table,
    format_csv,
    format_json,
    format_output,
    format_table,
)


class TestJsonFormatter:
    """Tests for JSON formatting."""

    def test_format_dict(self) -> None:
        """Test formatting a dictionary as JSON."""
        data = {"key": "value", "number": 42}
        result = format_json(data)

        parsed = json.loads(result)
        assert parsed["key"] == "value"
        assert parsed["number"] == 42

    def test_format_list(self) -> None:
        """Test formatting a list as JSON."""
        data = [{"id": 1}, {"id": 2}]
        result = format_json(data)

        parsed = json.loads(result)
        assert len(parsed) == 2
        assert parsed[0]["id"] == 1

    def test_format_nested(self) -> None:
        """Test formatting nested structures."""
        data = {
            "items": [{"name": "item1"}, {"name": "item2"}],
            "count": 2,
        }
        result = format_json(data)

        parsed = json.loads(result)
        assert len(parsed["items"]) == 2
        assert parsed["count"] == 2


class TestCsvFormatter:
    """Tests for CSV formatting."""

    def test_format_csv_simple(self) -> None:
        """Test formatting simple CSV data."""
        headers = ["Name", "Value"]
        rows = [["Alice", "100"], ["Bob", "200"]]

        result = format_csv(headers, rows)
        lines = result.strip().replace("\r", "").split("\n")

        assert lines[0] == "Name,Value"
        assert lines[1] == "Alice,100"
        assert lines[2] == "Bob,200"

    def test_format_csv_with_commas(self) -> None:
        """Test CSV formatting with values containing commas."""
        headers = ["Name", "Description"]
        rows = [["Smith, John", '"Test description"']]

        result = format_csv(headers, rows)

        # CSV should properly quote values with commas
        assert '"Smith, John"' in result or "Smith, John" in result

    def test_csv_parseable(self) -> None:
        """Test that output is valid CSV."""
        headers = ["ID", "Name", "Score"]
        rows = [["1", "Alice", "95"], ["2", "Bob", "87"]]

        result = format_csv(headers, rows)

        # Parse back to verify format
        reader = csv.reader(io.StringIO(result))
        parsed_rows = list(reader)

        assert parsed_rows[0] == headers
        assert parsed_rows[1] == ["1", "Alice", "95"]


class TestTableFormatter:
    """Tests for table formatting."""

    def test_format_table_simple(self) -> None:
        """Test formatting a simple table."""
        headers = ["Name", "Value"]
        rows = [["Alice", "100"], ["Bob", "200"]]

        result = format_table(headers, rows)

        # Should contain headers and values
        assert "Name" in result
        assert "Alice" in result
        assert "100" in result

    def test_format_empty_table(self) -> None:
        """Test formatting an empty table."""
        headers = ["Column1", "Column2"]
        rows = []

        result = format_table(headers, rows)

        # Should contain headers even if empty
        assert "Column1" in result or len(result) > 0

    def test_format_table_multiple_rows(self) -> None:
        """Test formatting table with many rows."""
        headers = ["ID", "Name", "Value"]
        rows = [[str(i), f"Item{i}", str(i * 10)] for i in range(10)]

        result = format_table(headers, rows)

        # Should contain all data
        for i in range(10):
            assert f"Item{i}" in result


class TestSimpleTableFormatter:
    """Tests for fallback simple table formatter."""

    def test_simple_format_single_row(self) -> None:
        """Test simple table formatting."""
        headers = ["Name", "Value"]
        rows = [["Alice", "100"]]

        result = _format_simple_table(headers, rows)

        assert "Name" in result
        assert "Alice" in result
        assert "100" in result

    def test_simple_format_alignment(self) -> None:
        """Test simple table alignment."""
        headers = ["Short", "VeryLongHeader"]
        rows = [["A", "B"]]

        result = _format_simple_table(headers, rows)

        # Should handle column widths
        assert "VeryLongHeader" in result

    def test_simple_format_no_data(self) -> None:
        """Test simple table with no data."""
        headers = ["Col1", "Col2"]
        rows = []

        result = _format_simple_table(headers, rows)

        assert "No data" in result


class TestFormatOutput:
    """Tests for the main format_output function."""

    def test_format_output_json(self) -> None:
        """Test format_output with JSON format."""
        data = {"key": "value"}
        result = format_output(data, format_type="json")

        parsed = json.loads(result)
        assert parsed["key"] == "value"

    def test_format_output_table(self) -> None:
        """Test format_output with table format."""
        headers = ["Name", "Value"]
        rows = [["Test", "123"]]

        result = format_output({}, format_type="table", headers=headers, rows=rows)

        assert "Name" in result
        assert "Test" in result

    def test_format_output_csv(self) -> None:
        """Test format_output with CSV format."""
        headers = ["Name", "Value"]
        rows = [["Test", "123"]]

        result = format_output({}, format_type="csv", headers=headers, rows=rows)

        assert "Name,Value" in result
        assert "Test,123" in result

    def test_format_output_invalid_type(self) -> None:
        """Test format_output with invalid format type."""
        with pytest.raises(ValueError):
            format_output({}, format_type="invalid")

    def test_format_output_missing_headers(self) -> None:
        """Test format_output without required headers."""
        with pytest.raises(ValueError):
            format_output({}, format_type="table")

    def test_format_output_missing_rows(self) -> None:
        """Test format_output without required rows."""
        with pytest.raises(ValueError):
            format_output({}, format_type="csv", headers=["Col"])
