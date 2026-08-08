# Getting Started with FraiseQL

A 10-minute guide: author a schema, create the database objects, compile, and
serve your first GraphQL API.

Every command in this guide is executed verbatim by CI against a real
PostgreSQL (`tools/quickstart-smoke.sh`), so if it is printed here, it works.

## Prerequisites

- **Rust** 1.94+ (install via [rustup](https://rustup.rs))
- **PostgreSQL** 14+ running locally (or Docker: `docker run -p 5432:5432 -e POSTGRES_PASSWORD=postgres postgres:16`)
- **Python 3.11+** for schema authoring

## 1. Install the CLI and Server

```bash
cargo install fraiseql-cli fraiseql-server
```

## 2. Define Your Schema (Python)

```bash
pip install fraiseql
```

Create `schema.py`:

```python
import fraiseql
from fraiseql import ID


@fraiseql.type(sql_source="v_user")
class User:
    id: ID
    name: str
    email: str


@fraiseql.query(sql_source="v_user")
def users() -> list[User]:
    """List all users."""
    ...


if __name__ == "__main__":
    fraiseql.export_schema("schema.json")
```

```bash
python schema.py
```

This produces `schema.json` — a declarative description of your types and
their SQL sources.

## 3. Create the Database Objects

FraiseQL reads every entity through a view that exposes a single JSONB `data`
column (`SELECT data FROM v_user` is the exact SQL the runtime issues). The
JSONB keys are snake_case; the runtime maps your schema's camelCase field
names onto them.

Create `setup.sql`:

```sql
CREATE TABLE tb_user (
    pk_user  SERIAL PRIMARY KEY,
    id       UUID NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    name     TEXT NOT NULL,
    email    TEXT NOT NULL UNIQUE
);

CREATE VIEW v_user AS
SELECT
    pk_user,
    id,
    jsonb_build_object('id', id, 'name', name, 'email', email) AS data
FROM tb_user;

INSERT INTO tb_user (name, email) VALUES ('Ada', 'ada@example.com');
```

Apply it:

```bash
export DATABASE_URL="postgres://postgres:postgres@localhost:5432/postgres"
psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -f setup.sql
```

## 4. Compile the Schema

```bash
fraiseql-cli compile schema.json -o schema.compiled.json
```

The compiler validates your schema, generates optimized SQL templates, and
produces `schema.compiled.json`.

## 5. Start the Server

```bash
export FRAISEQL_ENV=development
fraiseql-server --schema-path schema.compiled.json
```

Your GraphQL endpoint is now live at `http://localhost:8000/graphql` (the
default bind address is `127.0.0.1:8000`; change it with `--bind-addr`).

`FRAISEQL_ENV=development` relaxes the production boot checks (for example,
CORS origin validation). In production, leave it unset and configure
`fraiseql.toml` instead — the server fails closed on unsafe defaults.

## 6. Query

```bash
curl -X POST http://localhost:8000/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ users { id name email } }"}'
```

Expected response:

```json
{"data":{"users":[{"id":"…","name":"Ada","email":"ada@example.com"}]}}
```

## Mutations Need One More Step

Two hard runtime requirements apply to every state-changing mutation:

1. **`fraiseql setup`** must have been run against the database. It installs
   the `fraiseql.mutation_ok` / `fraiseql.mutation_err` response builders and
   the `core.tb_entity_change_log` change-log outbox table the mutation
   executor writes in-transaction. Without it, the first mutation fails at
   prepare time.
2. **Every mutation function must return the 13-column v2.2
   `mutation_response` row** (build it with the installed helpers).
   `RETURNS SETOF v_*` and bare scalar returns are not supported.

See [mutation-response.md](../architecture/mutation-response.md) for the
contract, or run `fraiseql init my-app` for a complete scaffold whose views,
functions and printed next steps already follow it.

## Next Steps

- [Architecture Documentation](../architecture/README.md) — Understand the compilation pipeline
- [Configuration Reference](../../fraiseql.toml.example) — All server and security options
- [Security Checklist](production-security-checklist.md) — Harden for production
- [Roadmap](../../roadmap.md) — What's coming next
