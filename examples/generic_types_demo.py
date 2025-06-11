"""Demo of generic types support in FraiseQL.

This example shows how to use generic types like Connection[T], Edge[T], and
PaginatedResponse[T] for type-safe pagination patterns.
"""

import fraiseql
from fraiseql.core.graphql_type import convert_type_to_graphql_output


@fraiseql.type
class User:
    """User model for the demo."""

    id: str = fraiseql.fraise_field(description="User ID")
    name: str = fraiseql.fraise_field(description="User name")
    email: str = fraiseql.fraise_field(description="User email")


@fraiseql.type
class Post:
    """Post model for the demo."""

    id: str = fraiseql.fraise_field(description="Post ID")
    title: str = fraiseql.fraise_field(description="Post title")
    content: str = fraiseql.fraise_field(description="Post content")
    author_id: str = fraiseql.fraise_field(description="Author user ID")


def demo_generic_types():
    """Demonstrate generic type support."""

    print("=== FraiseQL Generic Types Demo ===\n")

    # Import the generic types
    from fraiseql import Connection, Edge, PageInfo, PaginatedResponse

    print("1. Generic types are available:")
    print(f"   - Connection: {Connection}")
    print(f"   - Edge: {Edge}")
    print(f"   - PageInfo: {PageInfo}")
    print(f"   - PaginatedResponse: {PaginatedResponse} (alias for Connection)")
    print()

    print("2. Creating concrete types from generics:")

    # User connection
    user_connection_type = convert_type_to_graphql_output(Connection[User])
    print(f"   - Connection[User] -> GraphQL type: {user_connection_type.name}")
    print(f"     Fields: {list(user_connection_type.fields.keys())}")

    # Post connection
    post_connection_type = convert_type_to_graphql_output(Connection[Post])
    print(f"   - Connection[Post] -> GraphQL type: {post_connection_type.name}")
    print(f"     Fields: {list(post_connection_type.fields.keys())}")

    # Edge types
    user_edge_type = convert_type_to_graphql_output(Edge[User])
    print(f"   - Edge[User] -> GraphQL type: {user_edge_type.name}")
    print(f"     Fields: {list(user_edge_type.fields.keys())}")

    print()

    print("3. PaginatedResponse alias works:")
    paginated_users = convert_type_to_graphql_output(PaginatedResponse[User])
    print(f"   - PaginatedResponse[User] -> {paginated_users.name}")
    print(
        f"   - Same as Connection[User]: {paginated_users.name == user_connection_type.name}"
    )
    print()

    print("4. Generated GraphQL schema structure:")
    print(f"""
   type {user_connection_type.name} {{
     edges: [EdgeUser!]!
     page_info: PageInfo!
     total_count: Int
   }}

   type EdgeUser {{
     node: User!
     cursor: String!
   }}

   type PageInfo {{
     has_next_page: Boolean!
     has_previous_page: Boolean!
     start_cursor: String
     end_cursor: String
     total_count: Int
   }}

   type User {{
     id: String!
     name: String!
     email: String!
   }}
    """)

    print("5. Type safety benefits:")
    print("   ✅ Connection[User] and Connection[Post] are different types")
    print("   ✅ TypeScript-like generic type checking in Python")
    print("   ✅ Automatic GraphQL schema generation")
    print("   ✅ Reusable pagination patterns")
    print("   ✅ Compatible with printoptim's PaginatedResponse[T] pattern")

    print("\n6. Usage in resolvers (example):")
    print("""
   @fraiseql.field
   async def users(
       first: int = 20,
       after: str = None
   ) -> Connection[User]:
       # Your pagination logic here
       return Connection(
           edges=[Edge(node=user, cursor="cursor") for user in users],
           page_info=PageInfo(has_next_page=True, has_previous_page=False),
           total_count=100
       )
    """)


if __name__ == "__main__":
    demo_generic_types()
