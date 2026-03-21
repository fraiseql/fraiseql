# Getting Started with FraiseQL — Video Tutorial Outline

**Target duration**: 8 minutes
**Audience**: Backend developers familiar with GraphQL but new to FraiseQL
**Goal**: From zero to running GraphQL server with auth in under 10 minutes

---

## 1. Intro (30s)

- What is FraiseQL: a compiled GraphQL execution engine
- Key differentiator: schema → SQL at build time, zero runtime overhead
- Three layers: Author (Python/TS) → Compile (Rust) → Serve (Rust)

## 2. Install (1m)

- Install the CLI: `cargo install fraiseql-cli`
- Install the Python SDK: `uv pip install fraiseql`
- Verify: `fraiseql-cli --version`

## 3. Define a Schema (2m)

- Create `schema.py` with Python decorators
- `@fraiseql.type` — define a User type with fields
- `@fraiseql.mutation` — add a `createUser` mutation with `sql_source`
- `@fraiseql.subscription` — add a real-time subscription
- Show `fraiseql.field()` with `requires_scope` for field-level auth
- Run `python schema.py` to generate `schema.json`

## 4. Compile (1m)

- Run `fraiseql-cli compile schema.json`
- Show the output `schema.compiled.json` — types + SQL templates + config
- Point out: SQL is generated at build time, not runtime
- Show `fraiseql-cli explain "{ users { id name } }"` for query cost analysis

## 5. Run the Server (1m)

- Run `fraiseql-cli serve --schema schema.compiled.json`
- Open GraphiQL in browser at `http://localhost:8080/graphiql`
- Show the auto-generated schema explorer

## 6. Query (1m)

- Simple query: `{ users { id name email } }`
- Mutation: `mutation { createUser(name: "Alice") { id } }`
- Relay pagination: `{ users(first: 10, after: "cursor") { edges { node { id } } pageInfo { hasNextPage } } }`
- Show variables panel in GraphiQL

## 7. Add Authentication (1m)

- Create `fraiseql.toml` with JWT auth config
- Show `[fraiseql.security.rate_limiting]` section
- Re-compile with config: `fraiseql-cli compile schema.json`
- Re-run server, show that protected fields now require a token
- Demo with `Authorization: Bearer <token>` header

## 8. Deploy (30s)

- Show `docker-compose.yml` with FraiseQL + PostgreSQL
- `docker-compose up` — production-ready in one command
- Mention: OpenAPI spec at `/api/v1/openapi.json`
- Link to full documentation

---

## Production Notes (post-credits / description box)

- REST API for admin operations: schema reload, cache clear, metrics
- Federation support for microservice architectures
- Field-level encryption, audit logging, RBAC
- See `docs/architecture/overview.md` for deep dive
