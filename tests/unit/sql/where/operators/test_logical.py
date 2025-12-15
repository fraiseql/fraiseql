"""Comprehensive tests for logical operator SQL building."""

from psycopg.sql import SQL, Composed

from fraiseql.sql.where.operators.logical import build_and_sql, build_not_sql, build_or_sql


class TestLogicalAndOperator:
    """Test AND operator combinations."""

    def test_and_empty_conditions(self):
        """Test AND with empty conditions list."""
        result = build_and_sql([])
        sql_str = result.as_string(None)
        assert sql_str == "TRUE"

    def test_and_single_condition(self):
        """Test AND with single condition."""
        condition = Composed([SQL("age > "), SQL("18")])
        result = build_and_sql([condition])
        sql_str = result.as_string(None)
        assert sql_str == "age > 18"

    def test_and_two_conditions(self):
        """Test AND with two conditions."""
        condition1 = Composed([SQL("age > "), SQL("18")])
        condition2 = Composed([SQL("status = "), SQL("'active'")])
        result = build_and_sql([condition1, condition2])
        sql_str = result.as_string(None)
        assert "age > 18" in sql_str
        assert "status = 'active'" in sql_str
        assert " AND " in sql_str
        assert sql_str.startswith("(")
        assert sql_str.endswith(")")

    def test_and_multiple_conditions(self):
        """Test AND with multiple conditions."""
        conditions = [
            Composed([SQL("age >= "), SQL("21")]),
            Composed([SQL("country = "), SQL("'US'")]),
            Composed([SQL("verified = "), SQL("TRUE")]),
        ]
        result = build_and_sql(conditions)
        sql_str = result.as_string(None)
        assert "age >= 21" in sql_str
        assert "country = 'US'" in sql_str
        assert "verified = TRUE" in sql_str
        assert sql_str.count(" AND ") == 2


class TestLogicalOrOperator:
    """Test OR operator combinations."""

    def test_or_empty_conditions(self):
        """Test OR with empty conditions list."""
        result = build_or_sql([])
        sql_str = result.as_string(None)
        assert sql_str == "FALSE"

    def test_or_single_condition(self):
        """Test OR with single condition."""
        condition = Composed([SQL("status = "), SQL("'draft'")])
        result = build_or_sql([condition])
        sql_str = result.as_string(None)
        assert sql_str == "status = 'draft'"

    def test_or_two_conditions(self):
        """Test OR with two conditions."""
        condition1 = Composed([SQL("status = "), SQL("'draft'")])
        condition2 = Composed([SQL("status = "), SQL("'published'")])
        result = build_or_sql([condition1, condition2])
        sql_str = result.as_string(None)
        assert "status = 'draft'" in sql_str
        assert "status = 'published'" in sql_str
        assert " OR " in sql_str
        assert sql_str.startswith("(")
        assert sql_str.endswith(")")

    def test_or_multiple_conditions(self):
        """Test OR with multiple conditions."""
        conditions = [
            Composed([SQL("role = "), SQL("'admin'")]),
            Composed([SQL("role = "), SQL("'moderator'")]),
            Composed([SQL("role = "), SQL("'editor'")]),
        ]
        result = build_or_sql(conditions)
        sql_str = result.as_string(None)
        assert "role = 'admin'" in sql_str
        assert "role = 'moderator'" in sql_str
        assert "role = 'editor'" in sql_str
        assert sql_str.count(" OR ") == 2


class TestLogicalNotOperator:
    """Test NOT operator negation."""

    def test_not_simple_condition(self):
        """Test NOT with simple condition."""
        condition = Composed([SQL("active = "), SQL("TRUE")])
        result = build_not_sql(condition)
        sql_str = result.as_string(None)
        assert sql_str == "NOT (active = TRUE)"
        assert sql_str.startswith("NOT (")
        assert sql_str.endswith(")")

    def test_not_complex_condition(self):
        """Test NOT with complex condition."""
        condition = Composed([SQL("age > 18 AND status = 'active'")])
        result = build_not_sql(condition)
        sql_str = result.as_string(None)
        assert "NOT (" in sql_str
        assert "age > 18 AND status = 'active'" in sql_str
        assert sql_str.endswith(")")

    def test_not_nested_logical(self):
        """Test NOT with nested logical operations."""
        inner_and = build_and_sql(
            [Composed([SQL("x > "), SQL("0")]), Composed([SQL("y < "), SQL("100")])]
        )
        result = build_not_sql(inner_and)  # type: ignore
        sql_str = result.as_string(None)
        assert "NOT (" in sql_str
        assert "x > 0" in sql_str
        assert "y < 100" in sql_str
        assert " AND " in sql_str


class TestLogicalOperatorNesting:
    """Test complex nesting of logical operators."""

    def test_and_with_or_conditions(self):
        """Test AND containing OR conditions."""
        or_condition1 = build_or_sql(
            [
                Composed([SQL("status = "), SQL("'draft'")]),
                Composed([SQL("status = "), SQL("'review'")]),
            ]
        )
        or_condition2 = build_or_sql(
            [
                Composed([SQL("priority = "), SQL("'high'")]),
                Composed([SQL("priority = "), SQL("'urgent'")]),
            ]
        )

        result = build_and_sql([or_condition1, or_condition2])  # type: ignore[list-item]
        sql_str = result.as_string(None)
        assert "status = 'draft'" in sql_str
        assert "status = 'review'" in sql_str
        assert "priority = 'high'" in sql_str
        assert "priority = 'urgent'" in sql_str
        assert sql_str.count(" OR ") == 2
        assert " AND " in sql_str

    def test_or_with_nested_and_conditions(self):
        """Test OR containing AND conditions."""
        and_condition1 = build_and_sql(
            [Composed([SQL("age >= "), SQL("18")]), Composed([SQL("country = "), SQL("'US'")])]
        )
        and_condition2 = build_and_sql(
            [Composed([SQL("age >= "), SQL("21")]), Composed([SQL("country = "), SQL("'CA'")])]
        )

        result = build_or_sql([and_condition1, and_condition2])  # type: ignore[list-item]
        sql_str = result.as_string(None)
        assert "age >= 18" in sql_str
        assert "country = 'US'" in sql_str
        assert "age >= 21" in sql_str
        assert "country = 'CA'" in sql_str
        assert sql_str.count(" AND ") == 2
        assert " OR " in sql_str

    def test_not_with_and_or_combination(self):
        """Test NOT with AND/OR combination."""
        and_or = build_and_sql(
            [
                build_or_sql(
                    [Composed([SQL("x = "), SQL("1")]), Composed([SQL("x = "), SQL("2")])]
                ),
                Composed([SQL("y = "), SQL("3")]),
            ]
        )

        result = build_not_sql(and_or)  # type: ignore
        sql_str = result.as_string(None)
        assert sql_str.startswith("NOT (")
        assert "x = 1" in sql_str
        assert "x = 2" in sql_str
        assert "y = 3" in sql_str
        assert " OR " in sql_str
        assert " AND " in sql_str

    def test_or_with_and_conditions(self):
        """Test OR containing AND conditions."""
        and_condition1 = build_and_sql(
            [Composed([SQL("age >= "), SQL("18")]), Composed([SQL("country = "), SQL("'US'")])]
        )
        and_condition2 = build_and_sql(
            [Composed([SQL("age >= "), SQL("21")]), Composed([SQL("country = "), SQL("'CA'")])]
        )

        result = build_or_sql([and_condition1, and_condition2])  # type: ignore
        result_str = result.as_string(None)
        assert "age >= 18" in result_str
        assert "country = 'US'" in result_str
        assert "age >= 21" in result_str
        assert "country = 'CA'" in result_str
        assert result_str.count(" AND ") == 2
        assert " OR " in result_str

    def test_not_with_and_or(self):
        """Test NOT with AND/OR combination."""
        and_or = build_and_sql(
            [
                build_or_sql(
                    [Composed([SQL("x = "), SQL("1")]), Composed([SQL("x = "), SQL("2")])]
                ),
                Composed([SQL("y = "), SQL("3")]),
            ]
        )

        result = build_not_sql(and_or)  # type: ignore
        result_str = result.as_string(None)
        assert result_str.startswith("NOT (")
        assert "x = 1" in result_str
        assert "x = 2" in result_str
        assert "y = 3" in result_str
        assert " OR " in result_str
        assert " AND " in result_str

    def test_or_containing_and_conditions(self):
        """Test OR containing AND conditions."""
        and_condition1 = build_and_sql(
            [Composed([SQL("age >= "), SQL("18")]), Composed([SQL("country = "), SQL("'US'")])]
        )
        and_condition2 = build_and_sql(
            [Composed([SQL("age >= "), SQL("21")]), Composed([SQL("country = "), SQL("'CA'")])]
        )

        result = build_or_sql([and_condition1, and_condition2])  # type: ignore
        result_str = result.as_string(None)
        assert "age >= 18" in result_str
        assert "country = 'US'" in result_str
        assert "age >= 21" in result_str
        assert "country = 'CA'" in result_str
        assert result_str.count(" AND ") == 2
        assert " OR " in result_str

    def test_not_with_complex_and_or(self):
        """Test NOT with AND/OR combination."""
        and_or = build_and_sql(
            [
                build_or_sql(
                    [Composed([SQL("x = "), SQL("1")]), Composed([SQL("x = "), SQL("2")])]
                ),
                Composed([SQL("y = "), SQL("3")]),
            ]
        )

        result = build_not_sql(and_or)  # type: ignore
        result_str = result.as_string(None)
        assert result_str.startswith("NOT (")
        assert "x = 1" in result_str
        assert "x = 2" in result_str
        assert "y = 3" in result_str
        assert " OR " in result_str
        assert " AND " in result_str


class TestLogicalOperatorEdgeCases:
    """Test edge cases for logical operators."""

    def test_deep_nesting(self):
        """Test deeply nested logical operations."""
        # NOT(AND(OR(a, b), OR(c, d)))
        inner_or1 = build_or_sql([Composed([SQL("a")]), Composed([SQL("b")])])
        inner_or2 = build_or_sql([Composed([SQL("c")]), Composed([SQL("d")])])
        inner_and = build_and_sql([inner_or1, inner_or2])
        result = build_not_sql(inner_and)

        result_str = result.as_string(None)
        assert result_str.count("(") >= 4  # Multiple nesting levels
        assert result_str.count(")") >= 4
        assert "NOT (" in result_str
        assert " AND " in result_str
        assert result_str.count(" OR ") == 2

    def test_mixed_condition_types(self):
        """Test mixing different types of conditions."""
        conditions = [
            Composed([SQL("id = "), SQL("123")]),
            Composed([SQL("name LIKE "), SQL("'test%'")]),
            Composed([SQL("created_at > "), SQL("'2024-01-01'")]),
        ]
        result = build_and_sql(conditions)
        sql_str = result.as_string(None)
        assert "id = 123" in sql_str
        assert "name LIKE 'test%'" in sql_str
        assert "created_at > '2024-01-01'" in sql_str
        assert sql_str.count(" AND ") == 2
