"""Module for generating SQL ORDER BY clauses dynamically.

This module defines the `OrderBySet` dataclass, which aggregates multiple
ORDER BY instructions and compiles them into a PostgreSQL-safe SQL fragment
using the `psycopg` library's SQL composition utilities.

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
    """Missing."""

    field: str
    direction: OrderDirection = "asc"

    def to_sql(self) -> sql.Composed:
        """Missing."""
        path = self.field.split(".")
        json_path = sql.SQL(" -> ").join(sql.Literal(p) for p in path[:-1])
        last_key = sql.Literal(path[-1])
        if path[:-1]:
            data_expr = sql.SQL("data -> ") + json_path + sql.SQL(" ->> ") + last_key
        else:
            data_expr = sql.SQL("data ->> ") + last_key

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
