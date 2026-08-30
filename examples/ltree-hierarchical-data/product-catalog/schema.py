#!/usr/bin/env python3
"""Product catalog authored with the FraiseQL Python SDK.

Demonstrates PostgreSQL LTREE for category taxonomies: one `category_path`
column places each product in the tree, and "everything under Electronics",
"every top-level category", "this exact shelf" are WHERE filters rather than
a recursive CTE or a closure table.

The view it names is created by setup.sql (singular Trinity naming:
tb_product/v_product, the view exposing pk_product, id and a JSONB `data`
column).

Run: python3 schema.py
Output: schema.json (consumed by `fraiseql-cli compile`)

Requires the SDK: `pip install fraiseql` (or, inside this repository,
`pip install -e sdks/official/fraiseql-python`).
"""

from pathlib import Path

import fraiseql
from fraiseql import ID, DateTime, Decimal
from fraiseql.scalars import LTree


@fraiseql.type(sql_source="v_product")
class Product:
    """A product, placed in the category tree by `category_path`."""

    id: ID
    name: str
    description: str
    price: Decimal
    category_path: LTree
    sku: str
    in_stock: bool
    created_at: DateTime


@fraiseql.input
class CategoryPathFilter:
    """LTREE operators over `category_path`.

    These are declared rather than derived. FraiseQL does not auto-derive the
    ltree operator family onto a field: a declared field type cannot say whether
    the column behind it is really an `ltree`, and deriving a filter advertises
    an operator (#869). Declaring them is the author asserting that it is —
    setup.sql makes `category_path` an LTREE column, and the generated SQL casts
    the JSONB value with `::ltree` before applying the operator.

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

    Every WHERE field is `{operator: value}` — a bare `sku: "V15-DETECT"` is
    refused by the parser, so an ordinary column needs a filter type too.
    """

    eq: str | None = None
    neq: str | None = None
    contains: str | None = None


@fraiseql.input
class ProductWhere:
    """Filters for the `products` query."""

    category_path: CategoryPathFilter | None = None
    sku: StringFilter | None = None


@fraiseql.query(sql_source="v_product")
def products(where: ProductWhere | None = None) -> list[Product]:
    """Products, optionally filtered by position in the category tree.

    `{ products(where: {categoryPath: {descendantOf: "electronics"}}) }`
    returns every product under Electronics, at any depth.
    """
    ...


@fraiseql.query(sql_source="v_product")
def product(id: ID) -> Product | None:
    """One product by id."""
    ...


if __name__ == "__main__":
    output_path = Path(__file__).parent / "schema.json"
    fraiseql.export_schema(str(output_path))
    print(f"Schema exported to: {output_path}")
