"""Test vector distance ORDER BY SQL generation (TDD Red Cycle).

These tests verify that vector distance operators work correctly in ORDER BY clauses.
"""

import pytest

from fraiseql.sql.order_by_generator import OrderBy, OrderDirection


@pytest.mark.unit
def test_order_by_cosine_distance() -> None:
    """Test ORDER BY with cosine distance generates correct SQL."""
    # This test will fail initially - need to implement vector distance ORDER BY
    ob = OrderBy(
        field="embedding.cosine_distance", direction=OrderDirection.ASC, value=[0.1, 0.2, 0.3]
    )
    result = ob.to_sql().as_string(None)
    expected = "(t.\"embedding\") <=> '[0.1,0.2,0.3]'::vector ASC"
    assert result == expected


def test_order_by_l2_distance() -> None:
    """Test ORDER BY with L2 distance generates correct SQL."""
    ob = OrderBy(field="embedding.l2_distance", direction=OrderDirection.ASC, value=[0.1, 0.2, 0.3])
    result = ob.to_sql().as_string(None)
    expected = "(t.\"embedding\") <-> '[0.1,0.2,0.3]'::vector ASC"
    assert result == expected


def test_order_by_inner_product() -> None:
    """Test ORDER BY with inner product generates correct SQL."""
    ob = OrderBy(
        field="embedding.inner_product", direction=OrderDirection.ASC, value=[0.1, 0.2, 0.3]
    )
    result = ob.to_sql().as_string(None)
    expected = "(t.\"embedding\") <#> '[0.1,0.2,0.3]'::vector ASC"
    assert result == expected


def test_order_by_vector_asc_default() -> None:
    """Test that ASC is default direction for vector distance ordering."""
    ob = OrderBy(field="text_embedding.cosine_distance", value=[0.1, 0.2])
    result = ob.to_sql().as_string(None)
    expected = "(t.\"text_embedding\") <=> '[0.1,0.2]'::vector ASC"
    assert result == expected


def test_order_by_vector_desc() -> None:
    """Test DESC direction for vector distance ordering."""
    ob = OrderBy(field="embedding.cosine_distance", direction=OrderDirection.DESC, value=[0.1, 0.2])
    result = ob.to_sql().as_string(None)
    expected = "(t.\"embedding\") <=> '[0.1,0.2]'::vector DESC"
    assert result == expected
