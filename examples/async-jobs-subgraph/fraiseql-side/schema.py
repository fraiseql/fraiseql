"""FraiseQL-side schema for the async-jobs federation example.

The SQL half of the demo. It owns a single `User` entity (resolved from
`v_user` in PostgreSQL) and is exposed as an Apollo Federation v2 subgraph.

The non-SQL `enqueueJob` / `jobStatus` operations live in the *other* subgraph
(`../subgraph/`, written in Rust + async-graphql). A federation router composes
both into one GraphQL endpoint — see this directory's `../README.md`.
"""

from fraiseql import type, key


@type
@key("id")
class User:
    """User entity, owned by the FraiseQL (SQL-backed) subgraph."""

    id: str
    name: str
    email: str


@type
class Query:
    """Root query type for the FraiseQL subgraph."""

    def user(self, id: str) -> User | None:
        """Fetch a single user by ID (resolved from `v_user`)."""
        # FraiseQL resolves this from the database at runtime.

    def users(self) -> list[User]:
        """List all users."""
        # FraiseQL resolves this from the database at runtime.
