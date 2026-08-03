# Vector Similarity Search (pgvector)

FraiseQL supports pgvector-backed similarity search on declared vector fields
(#386): top-K nearest-neighbour queries via the `nearest` argument, and
threshold-based distance filtering via the WHERE DSL. PostgreSQL only.

## Declaring a vector field

TOML:

```toml
[types.Doc.fields.embedding]
type = "Vector"
vector = { dimensions = 1536, index_type = "hnsw", distance_metric = "cosine" }
```

Authoring IR (what SDKs emit):

```json
{ "name": "embedding", "type": "Vector", "nullable": false,
  "vector_config": { "dimensions": 1536, "index_type": "hnsw", "distance_metric": "cosine" } }
```

`dimensions` is required; a `Vector` field without `vector_config` — or
`vector_config` on any other type — is a compile error. `--emit-ddl` produces a
dimensioned `vector(N)` column, `CREATE EXTENSION IF NOT EXISTS vector`, and the
declared HNSW/IVFFlat index.

## Storage contract

The backing view must expose the vector as a **native `vector(N)` column**
named after the field (snake_case) — that is what `nearest` orders by and what
makes ANN indexes usable. For threshold WHERE filtering, the vector is resolved
from `data->>'field'` like every other filter, so carry its text form
(`embedding::text`) in the `data` payload **or** register the field as an
indexed native column. A view can serve both:

```sql
CREATE VIEW v_doc AS
  SELECT id,
         jsonb_build_object('id', id, 'title', title, 'embedding', embedding::text) AS data,
         embedding
  FROM tb_doc;
```

Selection tip: don't select the `embedding` GraphQL field unless you want the
payload — it is the type's declared field for configuration purposes.

## Top-K search: the `nearest` argument

```graphql
query Similar($q: [Float!]!) {
  docs(nearest: { vector: $q, k: 10, metric: "cosine" }) { id title }
}
```

- Lowers to `ORDER BY "embedding" <op> '[…]'::vector LIMIT k` — the shape
  HNSW/IVFFlat indexes accelerate.
- `metric` is optional and defaults to the field's declared `distance_metric`
  (`cosine` → `<=>`, `l2` → `<->`, `inner_product` → `<#>`, most-similar first).
- The query vector's dimension is validated against the declared
  `vector_config.dimensions` with a named error.
- Composes with `where` (and RLS / tenant injection) — filters apply, then
  similarity ordering.
- Refused loudly on: relay queries, non-list queries, types without (or with
  more than one) vector field, and documents combining `nearest` with `limit`
  or `orderBy` (`k` is the page size).

## Threshold filtering: WHERE distance operators

```graphql
{ docs(where: { embedding: { cosine_distance: { vector: $q, threshold: 0.25 } } }) { id } }
```

- Operand shape is `{vector: [Float!], threshold: Float}` (the old bare-array
  shape never executed and is refused with the corrected shape in the error).
- `cosine_distance` / `l2_distance` / `l1_distance`: rows with distance ≤
  `threshold`.
- `inner_product`: rows with raw inner product ≥ `threshold` (pgvector's `<#>`
  returns the negated inner product; the negation is handled internally).
- `hamming_distance` / `jaccard_distance` are **refused**: pgvector defines them
  over binary (`bit`) vectors, which FraiseQL's float `Vector` type cannot
  declare.
- Performance: ANN indexes accelerate `ORDER BY … LIMIT` (the `nearest` shape),
  not bare threshold predicates — expect a scan on large tables, or combine
  with `nearest`.
