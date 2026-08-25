# Complex Queries

Queries past the shape of `{ things { id } }`. Five patterns, each executed and
printed:

1. **Nested selection** — `order → customer` and `order → items → product`, three
   levels deep in one statement.
2. **Variables** — the same document, bound differently, with no string
   interpolation anywhere.
3. **Filtering** — `where`, including through a nested object.
4. **Ordering and pagination** — `orderBy`, `limit`, `offset`.
5. **Two root fields in one operation** — one round trip, two results, with
   aliases.

## Run it

```bash
createdb ecommerce_example
psql -v ON_ERROR_STOP=1 -d ecommerce_example -f ../ecommerce/sql/setup.sql
export DATABASE_URL=postgresql://localhost/ecommerce_example

./run.sh
```

## What to read

**Nesting is a view concern, not an N+1.** FraiseQL does not resolve a nested field
with a second query. `v_order` builds `data->'customer'` with `jsonb_build_object`
and `data->'items'` with `jsonb_agg`, and the engine projects the selection set out
of that blob and recases the keys. Read `../ecommerce/sql/setup.sql` alongside
case 1.

**`where`, `orderBy`, `limit` and `offset` are auto-params.** The compiler adds
them to every list query, so nothing in `../ecommerce/schema.py` declares them —
declaring `limit` there would shadow the one that paginates with one that filters.

**`Ok` is not success.** Each case checks the in-band `errors` array before
printing `data`, and the program exits non-zero if any query did not resolve.

## Uses

`examples/ecommerce` — `Category`, `Product`, `Customer`, `Order`, `OrderItem`.
The seed data uses fixed UUIDs, so the ids in `src/main.rs` are stable across a
rebuild.
