"""FraiseQL-side schema for the async-jobs federation example.

The SQL half of the demo. It owns a single `User` entity (resolved from
`v_user` in PostgreSQL) and is exposed as an Apollo Federation v2 subgraph.

The non-SQL `enqueueJob` / `jobStatus` operations live in the *other* subgraph
(`../subgraph/`, written in Rust + async-graphql). A federation router composes
both into one GraphQL endpoint — see this directory's `../README.md`.

Authoring only. Running this file writes `types.json`, which `fraiseql compile`
merges with `fraiseql.toml`:

    python3 schema.py
    fraiseql compile fraiseql.toml --types types.json -o schema.compiled.json

The Dockerfile in this directory performs exactly those two steps.
"""

import fraiseql


@fraiseql.type
class User:
    """User entity, owned by the FraiseQL (SQL-backed) subgraph.

    What makes this an Apollo Federation v2 entity is the `[[federation.entities]]`
    block in fraiseql.toml, which names this type and its key fields. In the TOML
    workflow the type carries its shape and the TOML carries its role, so the key
    is declared once, next to the subgraph's own federation settings.
    """

    id: str
    name: str
    email: str


if __name__ == "__main__":
    # The TOML workflow: types come from here, everything else (queries,
    # federation, database) from fraiseql.toml.
    fraiseql.export_types("types.json")
    print("Wrote types.json")
