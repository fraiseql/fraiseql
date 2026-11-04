"""Tests for fraiseql.fact_table: fact-table authoring at parity with Go and TypeScript (#1797)."""

import pytest

import fraiseql
from fraiseql.registry import SchemaRegistry


@pytest.fixture(autouse=True)
def _clear_registry():
    SchemaRegistry.clear()
    yield
    SchemaRegistry.clear()


REVENUE = {"name": "revenue", "sql_type": "numeric", "nullable": False}


def test_a_minimal_fact_table_emits_only_the_required_keys():
    fraiseql.fact_table("tf_sales", measures=[REVENUE])

    assert SchemaRegistry.get_schema()["fact_tables"] == [
        {
            "table_name": "tf_sales",
            "measures": [REVENUE],
            "dimensions": {"name": "data", "paths": []},
            "denormalized_filters": [],
        }
    ]


def test_every_declared_key_reaches_the_schema():
    balance = {
        "name": "closing_balance",
        "sql_type": "numeric",
        "nullable": False,
        "additivity": {
            "kind": "semi_additive",
            "over": "day",
            "using": "last",
            "entity": ["account_id"],
        },
    }
    dimensions = {
        "name": "data",
        "paths": [{"name": "status", "json_path": "data->>'status'", "data_type": "text"}],
    }
    filters = [
        {"name": "account_id", "sql_type": "bigint", "indexed": True},
        {"name": "day", "sql_type": "date", "indexed": True},
        {"name": "org_path", "sql_type": "ltree", "indexed": True, "hierarchy": "org"},
    ]
    fraiseql.fact_table(
        "tf_account_day",
        measures=[balance],
        dimensions=dimensions,
        denormalized_filters=filters,
        type_name="Account",
        row_scope="relation",
        native_measures={"closing_balance": "closing_balance"},
        native_dimension_mapping={"status": "status_code"},
    )

    assert SchemaRegistry.get_schema()["fact_tables"] == [
        {
            "table_name": "tf_account_day",
            "type_name": "Account",
            "row_scope": "relation",
            "measures": [balance],
            "dimensions": dimensions,
            "denormalized_filters": filters,
            "native_measures": {"closing_balance": "closing_balance"},
            "native_dimension_mapping": {"status": "status_code"},
        }
    ]


def test_fact_tables_omitted_when_none_registered():
    assert "fact_tables" not in SchemaRegistry.get_schema()


def test_a_duplicate_table_name_is_refused():
    fraiseql.fact_table("tf_sales", measures=[REVENUE])
    with pytest.raises(ValueError, match="tf_sales"):
        fraiseql.fact_table("tf_sales", measures=[REVENUE])


def test_a_row_scope_other_than_relation_is_refused():
    with pytest.raises(ValueError, match="row_scope"):
        fraiseql.fact_table("tf_sales", measures=[REVENUE], row_scope="server")


TENANT = {"name": "tenant_id", "sql_type": "text", "indexed": True}


def test_inject_reaches_the_schema_as_structured_inject_params():
    fraiseql.fact_table(
        "tf_sales",
        measures=[REVENUE],
        denormalized_filters=[TENANT],
        inject={"tenant_id": "jwt:tid"},
    )

    table = SchemaRegistry.get_schema()["fact_tables"][0]
    assert table["inject_params"] == {"tenant_id": {"source": "jwt", "claim": "tid"}}


def test_an_optional_claim_cannot_filter_a_fact_table():
    with pytest.raises(ValueError, match="jwt:tid"):
        fraiseql.fact_table(
            "tf_sales",
            measures=[REVENUE],
            denormalized_filters=[TENANT],
            inject={"tenant_id": "jwt_optional:tid"},
        )


def test_an_inject_column_must_be_a_denormalized_filter():
    with pytest.raises(ValueError, match="owner_id"):
        fraiseql.fact_table(
            "tf_sales",
            measures=[REVENUE],
            denormalized_filters=[TENANT],
            inject={"owner_id": "jwt:sub"},
        )


def test_inject_beside_row_scope_is_refused():
    with pytest.raises(ValueError, match="declare one"):
        fraiseql.fact_table(
            "tf_sales",
            measures=[REVENUE],
            denormalized_filters=[TENANT],
            row_scope="relation",
            inject={"tenant_id": "jwt:tid"},
        )


def test_an_unknown_additivity_kind_is_refused():
    measure = {**REVENUE, "additivity": {"kind": "sometimes"}}
    with pytest.raises(ValueError, match="additivity"):
        fraiseql.fact_table("tf_sales", measures=[measure])


def test_clear_forgets_fact_tables():
    fraiseql.fact_table("tf_sales", measures=[REVENUE])
    SchemaRegistry.clear()
    assert "fact_tables" not in SchemaRegistry.get_schema()


def test_fact_table_is_public():
    assert "fact_table" in fraiseql.__all__
