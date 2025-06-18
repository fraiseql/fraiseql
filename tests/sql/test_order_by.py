from fraiseql.sql.order_by_generator import OrderBy, OrderBySet


def test_single_order_by():
    ob = OrderBy(field="email")
    result = ob.to_sql().as_string(None)
    assert result == "data ->> 'email' ASC"


def test_nested_order_by_desc():
    ob = OrderBy(field="profile.age", direction="desc")
    result = ob.to_sql().as_string(None)
    assert result == "data -> 'profile' ->> 'age' DESC"


def test_order_by_set_multiple():
    obs = OrderBySet(
        [
            OrderBy(field="profile.last_name", direction="asc"),
            OrderBy(field="createdAt", direction="desc"),
        ],
    )
    result = obs.to_sql().as_string(None)
    expected = (
        "ORDER BY data -> 'profile' ->> 'last_name' ASC, data ->> 'created_at' DESC"
    )
    assert result == expected
