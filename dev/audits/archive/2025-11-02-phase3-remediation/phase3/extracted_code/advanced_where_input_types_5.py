# Extracted from: docs/advanced/where_input_types.md
# Block number: 5
@fraiseql.query
async def active_users_in_department(info, department: str) -> list[User]:
    db = info.context["db"]

    # Create filter programmatically
    where_filter = UserWhereInput(is_active={"eq": True}, department={"eq": department})

    return await db.find("users", where=where_filter)


@fraiseql.query
async def users_by_age_range(info, min_age: int, max_age: int) -> list[User]:
    db = info.context["db"]

    # Complex programmatic filter
    where_filter = UserWhereInput(
        AND=[
            UserWhereInput(age={"gte": min_age}),
            UserWhereInput(age={"lte": max_age}),
            UserWhereInput(is_active={"eq": True}),
        ]
    )

    return await db.find("users", where=where_filter)
