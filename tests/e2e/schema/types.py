"""
Reference E2E test schema.

This file is used to generate schema.json for the E2E pipeline test.
Run with: uv run python tests/e2e/schema/types.py > tests/e2e/schema.json
"""

try:
    import fraiseql

    @fraiseql.type
    class Author:
        pk_author_id: int
        name: str
        email: str

    @fraiseql.type
    class Post:
        pk_post_id: int
        fk_author_id: int
        title: str
        body: str
        published: bool

    @fraiseql.query
    def authors() -> list[Author]: ...

    @fraiseql.query
    def posts(published_only: bool = True) -> list[Post]: ...

    @fraiseql.mutation
    def create_author(name: str, email: str) -> Author: ...

    @fraiseql.mutation
    def create_post(title: str, body: str, fk_author_id: int) -> Post: ...

    if __name__ == "__main__":
        import json
        import sys
        schema = fraiseql.generate_schema()
        json.dump(schema, sys.stdout, indent=2)

except ImportError:
    # fraiseql not installed — this is expected during Rust-only CI
    pass
