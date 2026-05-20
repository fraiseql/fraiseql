# Getting Started with FraiseQL

A 5-minute guide to compiling and serving your first GraphQL API.

## Prerequisites

- **Rust** 1.92+ (install via [rustup](https://rustup.rs))
- **PostgreSQL** 14+ running locally (or Docker: `docker run -p 5432:5432 -e POSTGRES_PASSWORD=postgres postgres:16`)
- **Python 3.11+** or **Node.js 20+** for schema authoring

## 1. Install the CLI

```bash
cargo install fraiseql-cli
```

## 2. Define Your Schema (Python)

```bash
pip install fraiseql
```

Create `schema.py`:

```python
import fraiseql

fraiseql.config(database="postgresql")

@fraiseql.type(sql_source="users")
class User:
    id: int
    name: str
    email: str

@fraiseql.type(sql_source="posts")
class Post:
    id: int
    title: str
    body: str
    fk_user: int

fraiseql.export_schema("schema.json")
```

```bash
python schema.py
```

This produces `schema.json` -- a declarative description of your types and their SQL sources.

## 3. Compile the Schema

```bash
fraiseql-cli compile schema.json -o schema.compiled.json
```

The compiler validates your schema, generates optimized SQL templates, and produces `schema.compiled.json`.

## 4. Start the Server

```bash
DATABASE_URL="postgres://postgres:postgres@localhost/mydb" \
  fraiseql-server --schema schema.compiled.json
```

Your GraphQL endpoint is now live at `http://localhost:3000/graphql`.

## 5. Query

```bash
curl -X POST http://localhost:3000/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ users { id name email } }"}'
```

Or open the built-in GraphQL Studio at `http://localhost:3000/studio`.

## Next Steps

- [Architecture Documentation](../architecture/README.md) -- Understand the compilation pipeline
- [Configuration Reference](../../fraiseql.toml.example) -- All server and security options
- [Security Checklist](production-security-checklist.md) -- Harden for production
- [Roadmap](../../roadmap.md) -- What's coming next
