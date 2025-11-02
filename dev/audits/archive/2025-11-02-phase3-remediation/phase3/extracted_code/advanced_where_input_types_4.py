# Extracted from: docs/advanced/where_input_types.md
# Block number: 4
@fraiseql.type(sql_source="posts")
class Post:
    id: UUID
    title: str
    author_id: UUID
    author: User  # Nested relationship


# Generate Where input for nested filtering
PostWhereInput = create_graphql_where_input(Post)
