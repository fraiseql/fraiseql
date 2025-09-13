"""Module for generating SQL ORDER BY clauses with proper JSONB handling.

This module defines the `OrderBySet` dataclass, which aggregates multiple
ORDER BY instructions and compiles them into a PostgreSQL-safe SQL fragment
using the `psycopg` library's SQL composition utilities.

IMPORTANT: This module uses JSONB extraction (data -> 'field') rather than
text extraction (data ->> 'field') to preserve proper numeric ordering.
This prevents lexicographic sorting bugs where "125.0" > "1234.53" because
"2" > "1" in string comparison.

Key Features:
- Uses `data -> 'field'` for type-preserving JSONB extraction
- Maintains PostgreSQL's native type comparison behavior
- Supports nested field paths like `data -> 'profile' -> 'age'`
- Prevents numeric ordering bugs in financial and statistical data

The generated SQL is intended for use in query building where sorting by
multiple columns or expressions is required, supporting seamless integration
with dynamic query generators.
"""

from collections.abc import Sequence
from dataclasses import dataclass
from typing import Literal

from psycopg import sql

OrderDirection = Literal["asc", "desc"]


@dataclass(frozen=True)
class OrderBy:
    """Single ORDER BY clause with JSONB type preservation.

    Generates PostgreSQL ORDER BY clauses using JSONB extraction (data -> 'field')
    to maintain proper type-based sorting. This ensures numeric fields are sorted
    numerically rather than lexicographically.

    Attributes:
        field: The field name or nested path (e.g., 'amount' or 'profile.age')
        direction: Sort direction ('asc' or 'desc')

    Examples:
        OrderBy('amount') -> "data -> 'amount' ASC"
        OrderBy('profile.age', 'desc') -> "data -> 'profile' -> 'age' DESC"
    """

    field: str
    direction: OrderDirection = "asc"

    def to_sql(self) -> sql.Composed:
        """Generate ORDER BY clause using JSONB numeric extraction.

        Uses data -> 'field' instead of data ->> 'field' to preserve proper
        numeric ordering. JSONB extraction (data->'field') maintains the
        original data type for comparison, while text extraction (data->>'field')
        converts everything to text causing lexicographic sorting.

        For nested fields like 'profile.age', uses:
        data -> 'profile' -> 'age' (all JSONB extraction)
        """
        path = self.field.split(".")
        json_path = sql.SQL(" -> ").join(sql.Literal(p) for p in path[:-1])
        last_key = sql.Literal(path[-1])
        if path[:-1]:
            # For nested fields: data -> 'profile' -> 'age' (all JSONB)
            data_expr = sql.SQL("data -> ") + json_path + sql.SQL(" -> ") + last_key
        else:
            # For simple fields: data -> 'field' (JSONB)
            data_expr = sql.SQL("data -> ") + last_key

        direction_sql = sql.SQL(self.direction.upper())
        return data_expr + sql.SQL(" ") + direction_sql


@dataclass(frozen=True)
class OrderBySet:
    """Represents a set of ORDER BY instructions for SQL query construction.

    Attributes:
        instructions: A sequence of `OrderBy` instances representing individual
            ORDER BY clauses to be combined.
    """

    instructions: Sequence[OrderBy]

    def to_sql(self) -> sql.Composed:
        """Compile the ORDER BY instructions into a psycopg SQL Composed object.

        Returns:
            A `psycopg.sql.Composed` instance representing the full ORDER BY
            clause. Returns an empty SQL fragment if no instructions exist.
        """
        if not self.instructions:
            return sql.Composed([])  # Return empty Composed to satisfy Pyright
        clauses = sql.SQL(", ").join(instr.to_sql() for instr in self.instructions)
        return sql.SQL("ORDER BY ") + clauses
