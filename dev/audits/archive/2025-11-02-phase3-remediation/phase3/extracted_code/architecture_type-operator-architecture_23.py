# Extracted from: docs/architecture/type-operator-architecture.md
# Block number: 23
def test_ltree_ancestor_of_operation(self):
    """Test LTree ancestor_of operation (@>)."""
    registry = get_operator_registry()
    path_sql = SQL("data->>'path'")

    sql = registry.build_sql(
        path_sql=path_sql, op="ancestor_of", val="departments.engineering.backend", field_type=LTree
    )

    sql_str = str(sql)
    assert "::ltree" in sql_str
    assert "@>" in sql_str
    assert "departments.engineering.backend" in sql_str
