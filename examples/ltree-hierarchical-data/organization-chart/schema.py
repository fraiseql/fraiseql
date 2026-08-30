#!/usr/bin/env python3
"""Organization chart authored with the FraiseQL Python SDK.

Demonstrates PostgreSQL LTREE for employee hierarchies: one `org_path` column
carries each employee's position in the tree, and the hierarchy questions —
"everyone under the CTO", "everyone at VP level" — are WHERE filters, not
recursive joins.

The view it names is created by setup.sql (singular Trinity naming:
tb_employee/v_employee, the view exposing pk_employee, id and a JSONB `data`
column).

Run: python3 schema.py
Output: schema.json (consumed by `fraiseql-cli compile`)

Requires the SDK: `pip install fraiseql` (or, inside this repository,
`pip install -e sdks/official/fraiseql-python`).
"""

from pathlib import Path

import fraiseql
from fraiseql import ID, Date, Decimal
from fraiseql.scalars import LTree


@fraiseql.type(sql_source="v_employee")
class Employee:
    """An employee, with their position in the organization tree."""

    id: ID
    name: str
    title: str
    department: str
    salary: Decimal
    org_path: LTree
    hire_date: Date
    active: bool
    manager_name: str | None


@fraiseql.input
class OrgPathFilter:
    """LTREE operators over `org_path`.

    These are declared rather than derived. FraiseQL does not auto-derive the
    ltree operator family onto a field: a declared type cannot say whether the
    column behind it is really an `ltree`, and deriving a filter advertises an
    operator (#869). Declaring them here is the author asserting that it is —
    setup.sql makes `org_path` an LTREE column, and the generated SQL casts the
    JSONB value with `::ltree` before applying the operator.

    The ID-valued variants (`descendantOfId`, `ancestorOfId`) are deliberately
    absent: they resolve a UUID to a path through a `[hierarchies.<name>]`
    section in fraiseql.toml, which this example does not configure.
    """

    eq: LTree | None = None
    descendant_of: LTree | None = None
    ancestor_of: LTree | None = None
    depth_eq: int | None = None


@fraiseql.input
class StringFilter:
    """Comparison operators over a text field.

    Every WHERE field is `{operator: value}` — a bare `department: "Engineering"`
    is refused by the parser, so an ordinary column needs a filter type too.
    """

    eq: str | None = None
    neq: str | None = None
    contains: str | None = None


@fraiseql.input
class EmployeeWhere:
    """Filters for the `employees` query."""

    org_path: OrgPathFilter | None = None
    department: StringFilter | None = None


@fraiseql.query(sql_source="v_employee")
def employees(where: EmployeeWhere | None = None) -> list[Employee]:
    """Employees, optionally filtered by position in the hierarchy.

    `{ employees(where: {orgPath: {descendantOf: "acme.technology"}}) }`
    returns everyone under Technology, at any depth.
    """
    ...


@fraiseql.query(sql_source="v_employee")
def employee(id: ID) -> Employee | None:
    """One employee by id."""
    ...


if __name__ == "__main__":
    output_path = Path(__file__).parent / "schema.json"
    fraiseql.export_schema(str(output_path))
    print(f"Schema exported to: {output_path}")
