"""A shared product catalog with localized labels, authored with the Python SDK.

    uv run --project ../../sdks/official/fraiseql-python python schema.py   # writes catalog.json

`catalog.json` is committed and included by `fraiseql.toml`;
`sdks/official/conformance/check_examples.sh` re-exports it and fails when the two differ.
"""

import fraiseql
from fraiseql.scalars import ID


@fraiseql.type(sql_source="tv_product")
class Product:
    """A product of the shared catalog. Its name is stored in every language at once."""

    id: ID
    sku: str
    name: fraiseql.Localized[str]


@fraiseql.type(sql_source="v_tenant_order")
class TenantOrder:
    """An order of the caller's tenant, labelled in the tenant's stored locale by the view."""

    id: ID
    quantity: int
    product_label: str | None


@fraiseql.query(sql_source="tv_product", auto_params=True, cache_ttl_seconds=60)
def products() -> list[Product]:
    """The catalog, labelled in the request locale."""


@fraiseql.query(sql_source="v_tenant_order", inject={"tenant_id": "jwt:tenant_id"})
def tenant_orders() -> list[TenantOrder]:
    """The caller's tenant's orders."""


@fraiseql.mutation(sql_source="fn_rename_product", operation="update")
def rename_product(id: ID, name: fraiseql.Localized[str]) -> Product:
    """Set one or more of a product's labels; the others are kept."""


if __name__ == "__main__":
    fraiseql.export_schema("catalog.json")
