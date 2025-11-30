import pytest

from fraiseql.sql.order_by_generator import OrderBy, OrderBySet, OrderDirection

pytestmark = [pytest.mark.integration, pytest.mark.database]


@pytest.mark.unit
def test_single_order_by() -> None:
    ob = OrderBy(field="email")
    result = ob.to_sql().as_string(None)
    # Updated to use table alias for proper type preservation
    assert result == "t -> 'email' ASC"


def test_nested_order_by_desc() -> None:
    ob = OrderBy(field="profile.age", direction=OrderDirection.DESC)
    result = ob.to_sql().as_string(None)
    # Updated to use table alias for nested fields
    assert result == "t -> 'profile' -> 'age' DESC"


def test_order_by_set_multiple() -> None:
    obs = OrderBySet(
        [
            OrderBy(field="profile.last_name", direction=OrderDirection.ASC),
            OrderBy(field="created_at", direction=OrderDirection.DESC),
        ]
    )
    result = obs.to_sql().as_string(None)
    # Updated to use table alias for all fields
    expected = "ORDER BY t -> 'profile' -> 'last_name' ASC, t -> 'created_at' DESC"
    assert result == expected
