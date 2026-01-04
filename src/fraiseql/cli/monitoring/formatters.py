"""Output formatters for monitoring CLI commands.

Provides formatting for multiple output formats:
- Table (ASCII table using tabulate)
- JSON (structured JSON output)
- CSV (comma-separated values)
"""

from __future__ import annotations

import csv
import io
import json
from typing import Any


def format_table(headers: list[str], rows: list[list[str]]) -> str:
    """Format data as ASCII table.

    Args:
        headers: Column headers
        rows: List of rows, each row is list of strings

    Returns:
        Formatted ASCII table string
    """
    try:
        from tabulate import tabulate

        return tabulate(rows, headers=headers, tablefmt="grid")
    except ImportError:
        # Fallback to simple format if tabulate not available
        return _format_simple_table(headers, rows)


def _format_simple_table(headers: list[str], rows: list[list[str]]) -> str:
    """Simple ASCII table formatter (fallback).

    Args:
        headers: Column headers
        rows: List of rows

    Returns:
        Formatted table string
    """
    if not rows:
        return "No data"

    # Calculate column widths
    col_widths = [len(h) for h in headers]
    for row in rows:
        for i, cell in enumerate(row):
            col_widths[i] = max(col_widths[i], len(str(cell)))

    # Build table
    lines = []

    # Header
    header_row = "  ".join(str(h).ljust(col_widths[i]) for i, h in enumerate(headers))
    lines.append(header_row)
    lines.append("-" * len(header_row))

    # Rows
    for row in rows:
        formatted_row = "  ".join(str(cell).ljust(col_widths[i]) for i, cell in enumerate(row))
        lines.append(formatted_row)

    return "\n".join(lines)


def format_json(data: dict[str, Any] | list[dict[str, Any]]) -> str:
    """Format data as JSON.

    Args:
        data: Dictionary or list of dictionaries

    Returns:
        JSON formatted string
    """
    return json.dumps(data, indent=2, default=str)


def format_csv(headers: list[str], rows: list[list[str]]) -> str:
    """Format data as CSV.

    Args:
        headers: Column headers
        rows: List of rows, each row is list of strings

    Returns:
        CSV formatted string
    """
    output = io.StringIO()
    writer = csv.writer(output)

    # Write header
    writer.writerow(headers)

    # Write rows
    for row in rows:
        writer.writerow(row)

    return output.getvalue().strip()


def format_output(
    data: dict[str, Any] | list[dict[str, Any]],
    format_type: str = "table",
    headers: list[str] | None = None,
    rows: list[list[str]] | None = None,
) -> str:
    """Format output based on format type.

    Args:
        data: Data to format (for JSON output)
        format_type: 'table', 'json', or 'csv'
        headers: Column headers (required for table/csv)
        rows: Row data (required for table/csv)

    Returns:
        Formatted output string

    Raises:
        ValueError: If format_type is invalid
    """
    if format_type == "json":
        return format_json(data)
    if format_type == "csv":
        if headers is None or rows is None:
            raise ValueError("headers and rows required for CSV format")
        return format_csv(headers, rows)
    if format_type == "table":
        if headers is None or rows is None:
            raise ValueError("headers and rows required for table format")
        return format_table(headers, rows)
    raise ValueError(f"Unknown format type: {format_type}. Must be 'table', 'json', or 'csv'")
