"""Demo of Interface support in FraiseQL.

This example shows how to use GraphQL interfaces for polymorphic queries
and shared field definitions across types.
"""

import asyncio
from datetime import datetime
from uuid import UUID

import psycopg
from graphql import graphql

import fraiseql
from fraiseql import CQRSRepository, build_fraiseql_schema

# Database connection settings (using the same as mutations demo)
DB_CONFIG = {
    "host": "localhost",
    "port": 5433,
    "dbname": "fraiseql_demo",
    "user": "fraiseql",
    "password": "fraiseql",
}


# Define interfaces
@fraiseql.interface
class Node:
    """Base interface for all entities with an ID."""

    id: UUID = fraiseql.fraise_field(description="Unique identifier")


@fraiseql.interface
class Timestamped:
    """Interface for entities with timestamps."""

    created_at: str = fraiseql.fraise_field(description="Creation timestamp")
    updated_at: str | None = fraiseql.fraise_field(description="Last update timestamp")


@fraiseql.interface
class Publishable:
    """Interface for content that can be published."""

    title: str = fraiseql.fraise_field(description="Title of the content")
    published: bool = fraiseql.fraise_field(
        description="Whether the content is published"
    )
    published_at: str | None = fraiseql.fraise_field(
        description="Publication timestamp"
    )


# Define types implementing interfaces
@fraiseql.type(implements=[Node, Timestamped])
class User:
    """User type implementing Node and Timestamped."""

    id: UUID
    name: str
    email: str
    role: str
    created_at: str
    updated_at: str | None = None


@fraiseql.type(implements=[Node, Timestamped, Publishable])
class Article:
    """Article type implementing Node, Timestamped, and Publishable."""

    id: UUID
    title: str
    content: str
    author_id: UUID
    published: bool = False
    published_at: str | None = None
    created_at: str
    updated_at: str | None = None
    tags: list[str] = fraiseql.fraise_field(default_factory=list)


@fraiseql.type(implements=[Node, Timestamped, Publishable])
class Page:
    """Page type implementing Node, Timestamped, and Publishable."""

    id: UUID
    title: str
    slug: str
    content: str
    parent_id: UUID | None = None
    published: bool = False
    published_at: str | None = None
    created_at: str
    updated_at: str | None = None
    order: int = 0


# Query root with interface fields
@fraiseql.type
class QueryRoot:
    node: Node | None = fraiseql.fraise_field(
        description="Get any node by ID", purpose="output"
    )
    recent_content: list[Publishable] = fraiseql.fraise_field(
        default_factory=list,
        description="Get recent published content",
        purpose="output",
    )
    search_nodes: list[Node] = fraiseql.fraise_field(
        default_factory=list, description="Search all nodes", purpose="output"
    )

    @staticmethod
    async def resolve_node(_root, info) -> Node | None:
        """Resolve any node by ID (polymorphic)."""
        # For now, let's hardcode the ID since FraiseQL doesn't support field arguments yet
        id = UUID("123e4567-e89b-12d3-a456-426614174000")
        db: CQRSRepository = info.context["db"]

        # Try each table that implements Node
        async with db.connection.cursor() as cursor:
            # First try users
            await cursor.execute("SELECT data FROM users WHERE id = %s", (id,))
            result = await cursor.fetchone()
            if result:
                return User(**result[0])

            # Then try articles
            await cursor.execute("SELECT data FROM articles WHERE id = %s", (id,))
            result = await cursor.fetchone()
            if result:
                return Article(**result[0])

            # Finally try pages
            await cursor.execute("SELECT data FROM pages WHERE id = %s", (id,))
            result = await cursor.fetchone()
            if result:
                return Page(**result[0])

        return None

    @staticmethod
    async def resolve_recent_content(_root, info) -> list[Publishable]:
        """Get recent published content (articles and pages)."""
        db: CQRSRepository = info.context["db"]
        limit = 5  # Hardcoded for now

        # Query both articles and pages
        articles_query = """
            SELECT data
            FROM articles
            WHERE data->>'published' = 'true'
            ORDER BY data->>'published_at' DESC
            LIMIT %s
        """

        pages_query = """
            SELECT data
            FROM pages
            WHERE data->>'published' = 'true'
            ORDER BY data->>'published_at' DESC
            LIMIT %s
        """

        # Execute both queries
        async with db.connection.cursor() as cursor:
            # Get articles
            await cursor.execute(articles_query, (limit,))
            article_rows = await cursor.fetchall()
            articles = [Article(**row[0]) for row in article_rows]

            # Get pages
            await cursor.execute(pages_query, (limit,))
            page_rows = await cursor.fetchall()
            pages = [Page(**row[0]) for row in page_rows]

        # Combine and sort by published_at
        all_content = articles + pages
        all_content.sort(key=lambda x: x.published_at or "", reverse=True)

        return all_content[:limit]

    @staticmethod
    async def resolve_search_nodes(_root, info) -> list[Node]:
        """Search all nodes with optional filter."""
        db: CQRSRepository = info.context["db"]
        nodes = []

        # Build filter
        where_clause = ""
        params = []
        created_after = None  # Hardcoded for now
        if created_after:
            where_clause = "WHERE data->>'created_at' > %s"
            params = [created_after]

        # Query all tables
        for table in ["users", "articles", "pages"]:
            query = f"SELECT data FROM {table} {where_clause} ORDER BY data->>'created_at' DESC"
            async with db.conn.cursor() as cursor:
                await cursor.execute(query, params)
                rows = await cursor.fetchall()

                # Instantiate correct type based on table
                if table == "users":
                    nodes.extend([User(**row[0]) for row in rows])
                elif table == "articles":
                    nodes.extend([Article(**row[0]) for row in rows])
                elif table == "pages":
                    nodes.extend([Page(**row[0]) for row in rows])

        # Sort all by created_at
        nodes.sort(key=lambda x: x.created_at, reverse=True)
        return nodes


async def setup_database():
    """Create tables and sample data for the demo."""
    async with await psycopg.AsyncConnection.connect(**DB_CONFIG) as conn:
        async with conn.cursor() as cursor:
            # Create tables
            await cursor.execute("""
                CREATE TABLE IF NOT EXISTS users (
                    id UUID PRIMARY KEY,
                    data JSONB NOT NULL,
                    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
                    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
                );

                CREATE TABLE IF NOT EXISTS articles (
                    id UUID PRIMARY KEY,
                    data JSONB NOT NULL,
                    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
                    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
                );

                CREATE TABLE IF NOT EXISTS pages (
                    id UUID PRIMARY KEY,
                    data JSONB NOT NULL,
                    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
                    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
                );
            """)

            # Clear existing data
            await cursor.execute("TRUNCATE users, articles, pages")

            # Insert sample data
            now = datetime.now(datetime.UTC).isoformat()

            # Users
            await cursor.execute(
                """
                INSERT INTO users (id, data) VALUES
                ('123e4567-e89b-12d3-a456-426614174000'::uuid, %s::jsonb),
                ('123e4567-e89b-12d3-a456-426614174001'::uuid, %s::jsonb)
            """,
                (
                    psycopg.types.json.Json(
                        {
                            "id": "123e4567-e89b-12d3-a456-426614174000",
                            "name": "Alice Author",
                            "email": "alice@example.com",
                            "role": "editor",
                            "created_at": now,
                            "updated_at": None,
                        }
                    ),
                    psycopg.types.json.Json(
                        {
                            "id": "123e4567-e89b-12d3-a456-426614174001",
                            "name": "Bob Builder",
                            "email": "bob@example.com",
                            "role": "admin",
                            "created_at": now,
                            "updated_at": None,
                        }
                    ),
                ),
            )

            # Articles
            await cursor.execute(
                """
                INSERT INTO articles (id, data) VALUES
                ('223e4567-e89b-12d3-a456-426614174000'::uuid, %s::jsonb),
                ('223e4567-e89b-12d3-a456-426614174001'::uuid, %s::jsonb),
                ('223e4567-e89b-12d3-a456-426614174002'::uuid, %s::jsonb)
            """,
                (
                    psycopg.types.json.Json(
                        {
                            "id": "223e4567-e89b-12d3-a456-426614174000",
                            "title": "Getting Started with FraiseQL",
                            "content": "FraiseQL is a powerful GraphQL-to-PostgreSQL translator...",
                            "author_id": "123e4567-e89b-12d3-a456-426614174000",
                            "published": True,
                            "published_at": now,
                            "created_at": now,
                            "updated_at": None,
                            "tags": ["tutorial", "graphql", "postgresql"],
                        }
                    ),
                    psycopg.types.json.Json(
                        {
                            "id": "223e4567-e89b-12d3-a456-426614174001",
                            "title": "Advanced Interface Patterns",
                            "content": "Interfaces in GraphQL allow for powerful polymorphic queries...",
                            "author_id": "123e4567-e89b-12d3-a456-426614174000",
                            "published": True,
                            "published_at": now,
                            "created_at": now,
                            "updated_at": None,
                            "tags": ["advanced", "interfaces", "patterns"],
                        }
                    ),
                    psycopg.types.json.Json(
                        {
                            "id": "223e4567-e89b-12d3-a456-426614174002",
                            "title": "Draft: Upcoming Features",
                            "content": "Here's what's coming next...",
                            "author_id": "123e4567-e89b-12d3-a456-426614174001",
                            "published": False,
                            "published_at": None,
                            "created_at": now,
                            "updated_at": None,
                            "tags": ["draft", "roadmap"],
                        }
                    ),
                ),
            )

            # Pages
            await cursor.execute(
                """
                INSERT INTO pages (id, data) VALUES
                ('323e4567-e89b-12d3-a456-426614174000'::uuid, %s::jsonb),
                ('323e4567-e89b-12d3-a456-426614174001'::uuid, %s::jsonb)
            """,
                (
                    psycopg.types.json.Json(
                        {
                            "id": "323e4567-e89b-12d3-a456-426614174000",
                            "title": "About FraiseQL",
                            "slug": "about",
                            "content": "FraiseQL is an innovative approach to GraphQL APIs...",
                            "parent_id": None,
                            "published": True,
                            "published_at": now,
                            "created_at": now,
                            "updated_at": None,
                            "order": 1,
                        }
                    ),
                    psycopg.types.json.Json(
                        {
                            "id": "323e4567-e89b-12d3-a456-426614174001",
                            "title": "Documentation",
                            "slug": "docs",
                            "content": "Welcome to the FraiseQL documentation...",
                            "parent_id": None,
                            "published": True,
                            "published_at": now,
                            "created_at": now,
                            "updated_at": None,
                            "order": 2,
                        }
                    ),
                ),
            )

            await conn.commit()
            print("✓ Database setup complete")


async def demo():
    """Run the interface demo."""
    print("=== FraiseQL Interface Demo ===\n")

    # Setup database
    await setup_database()

    # Connect to database
    async with await psycopg.AsyncConnection.connect(**DB_CONFIG) as conn:
        db = CQRSRepository(conn)

        # Build schema
        schema = build_fraiseql_schema(query_types=[QueryRoot])

        # 1. Query node by ID (polymorphic)
        print("\n1. Querying node by ID (polymorphic):")
        query1 = """
        query GetNode {
            node {
                id
                ... on User {
                    name
                    email
                    role
                }
                ... on Article {
                    title
                    content
                    published
                }
                ... on Page {
                    title
                    slug
                }
            }
        }
        """

        result1 = await graphql(
            schema, query1, root_value=QueryRoot, context_value={"db": db}
        )

        if result1.errors:
            print(f"✗ Errors: {result1.errors}")
        elif result1.data and result1.data["node"]:
            print("✓ Found node:")
            user = result1.data["node"]
            print("  Type: User")
            print(f"  Name: {user['name']}")
            print(f"  Email: {user['email']}")
        else:
            print("✗ No node found or no data returned")

        # 2. Query recent published content
        print("\n2. Querying recent published content (mixed types):")
        query2 = """
        query GetRecentContent {
            recent_content {
                title
                published
                published_at
                ... on Article {
                    tags
                    author_id
                }
                ... on Page {
                    slug
                    order
                }
            }
        }
        """

        result2 = await graphql(
            schema, query2, root_value=QueryRoot, context_value={"db": db}
        )

        if result2.errors:
            print(f"✗ Errors: {result2.errors}")
        elif result2.data and result2.data.get("recent_content"):
            print("✓ Recent content:")
            for item in result2.data["recent_content"]:
                item_type = "Article" if "tags" in item else "Page"
                print(f"  - {item['title']} ({item_type})")
                if item_type == "Article":
                    print(f"    Tags: {', '.join(item['tags'])}")
                else:
                    print(f"    Slug: /{item['slug']}")
        else:
            print("✗ No content found or no data returned")

        # 3. Search all nodes with timestamp interface
        print("\n3. Searching all nodes (using Timestamped interface):")
        query3 = """
        query SearchNodes {
            search_nodes {
                id
                ... on Timestamped {
                    created_at
                    updated_at
                }
                ... on User {
                    name
                }
                ... on Article {
                    title
                }
                ... on Page {
                    title
                }
            }
        }
        """

        result3 = await graphql(
            schema, query3, root_value=QueryRoot, context_value={"db": db}
        )

        if result3.errors:
            print(f"✗ Errors: {result3.errors}")
        elif result3.data and result3.data["searchNodes"]:
            print("✓ All nodes with timestamps:")
            for node in result3.data["searchNodes"][:5]:  # Show first 5
                # Determine type
                if "name" in node:
                    node_type = "User"
                    display = node["name"]
                else:
                    node_type = "Article" if "title" in node else "Page"
                    display = node.get("title", "Unknown")

                print(f"  - {display} ({node_type})")
                print(f"    Created: {node['created_at'][:19]}")
        else:
            print("✗ No nodes found or no data returned")

        # 4. Show schema introspection
        print("\n4. Schema introspection - interfaces:")
        introspection_query = """
        {
            __schema {
                types {
                    name
                    kind
                    interfaces {
                        name
                    }
                }
            }
        }
        """

        result4 = await graphql(schema, introspection_query)

        if result4.data:
            print("✓ Types implementing interfaces:")
            for type_info in result4.data["__schema"]["types"]:
                if type_info["kind"] == "OBJECT" and type_info["interfaces"]:
                    print(
                        f"  - {type_info['name']}: {[i['name'] for i in type_info['interfaces']]}"
                    )


if __name__ == "__main__":
    asyncio.run(demo())
