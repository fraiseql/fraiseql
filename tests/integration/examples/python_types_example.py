"""
Example: Python SDK generating minimal types.json for TOML-based workflow

This example shows how to use the Python FraiseQL SDK to:
1. Define GraphQL types with @type decorator
2. Export minimal types.json (types only, no queries/mutations)
3. Combine with fraiseql.toml for complete schema compilation

Usage:
    python python_types_example.py
    # Generates: types.json

Then compile with:
    fraiseql compile fraiseql.toml --types types.json
    # Generates: schema.compiled.json
"""

from fraiseql import FraiseQL, type as graphql_type


@graphql_type
class User:
    """User in the system"""
    id: str
    name: str
    email: str
    created_at: str


@graphql_type
class Post:
    """Blog post"""
    id: str
    title: str
    content: str
    author_id: str
    created_at: str


@graphql_type
class Comment:
    """Comment on a post"""
    id: str
    text: str
    post_id: str
    author_id: str
    created_at: str


def main():
    """Register types and export minimal types.json"""
    # Register all types
    FraiseQL.registerTypes([User, Post, Comment])

    # Export minimal types.json (types only, no queries/mutations/federation/security)
    FraiseQL.exportTypes("types.json", pretty=True)

    print("✅ Generated types.json")
    print("   Types: 3 (User, Post, Comment)")
    print("\n🎯 Next steps:")
    print("   1. fraiseql compile fraiseql.toml --types types.json")
    print("   2. This merges types.json with fraiseql.toml configuration")
    print("   3. Result: schema.compiled.json with types + all config")


if __name__ == "__main__":
    main()
