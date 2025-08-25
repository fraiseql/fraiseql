from fraiseql.sql.order_by_generator import OrderBy, OrderBySet
import pytest




@pytest.mark.unit
def test_single_order_by() -> None:
    ob = OrderBy(field="email")
    result = ob.to_sql().as_string(None)
    assert result == "data ->> 'email' ASC"


def test_nested_order_by_desc() -> None:
    ob = OrderBy(field="profile.age", direction="desc")
    result = ob.to_sql().as_string(None)
    assert result == "data -> 'profile' ->> 'age' DESC"


def test_order_by_set_multiple() -> None:
    obs = OrderBySet(
        [
            OrderBy(field="profile.last_name", direction="asc"),
            OrderBy(field="created_at", direction="desc"),
        ]
    )
    result = obs.to_sql().as_string(None)
    expected = "ORDER BY data -> 'profile' ->> 'last_name' ASC, data ->> 'created_at' DESC"
    assert result == expected
