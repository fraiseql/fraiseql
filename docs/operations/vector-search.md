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

All eleven official SDKs author it (#959), each in its own idiom, and the
cross-SDK conformance suite holds them to it — the `vector_fields` construct
compiles a type carrying all four vector field types plus a distance field, and
asserts every key survives:

```python
# Python
embedding: Annotated[Vector, fraiseql.field(
    vector_config=fraiseql.VectorConfig(dimensions=1536, index_type="ivf_flat", distance_metric="l2"))]
```

```typescript
// TypeScript
{ name: "embedding", type: "Vector", nullable: false,
  vectorConfig: { dimensions: 1536, indexType: "ivf_flat", distanceMetric: "l2" } }
```

```go
// Go
{Name: "embedding", Type: "Vector",
    Vector: fraiseql.NewVectorConfig(1536).WithIndex(fraiseql.IndexIVFFlat).WithMetric(fraiseql.MetricL2)}
```

The index type and the distance metric have compiler-side defaults (`hnsw`,
`cosine`), and every SDK writes them into the emitted `schema.json` even when the
author left them off — so the artifact says which index and which metric the
column will get rather than leaving it to a default nobody chose. What an SDK
does **not** carry is the table of which combinations pgvector actually defines;
that lives in the compiler, which refuses an unsupported one by name. A copy of
that table in eleven places is a copy that drifts.

## Storage contract

The backing view must expose the vector as a **native `vector(N)` column**
named after the field (snake_case). That is the whole contract, and **both**
paths read it: `nearest` orders by it, and since #1117 the threshold WHERE
operators compare against it too.

```sql
CREATE VIEW v_doc AS
  SELECT id,
         jsonb_build_object('id', id, 'title', title) AS data,
         embedding
  FROM tb_doc;
```

The vector no longer needs a text copy inside `data` for filtering. Carry one
**only if you want to select the field**, which is a separate question:

```sql
-- Only if `{ docs { embedding } }` should return the vector.
jsonb_build_object('id', id, 'title', title, 'embedding', embedding::text) AS data
```

At 1536 dimensions that copy is several KB per row, and before #1117 it was
mandatory: a threshold predicate read `(data->>'embedding')::vector` and re-parsed
every row's embedding out of text, at **122×** the cost of the column (measured
below). Dropping it is the reason to.

⚠ **Breaking, if your view carried only the text form.** A view with
`'embedding', embedding::text` in `data` and no native `embedding` column stops
filtering — the predicate now names a column that is not there. Such a view could
never use `nearest` either (that path refuses without the column), so it was
already outside this contract; add the column to the view's select list.

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
- Refused loudly on: relay queries, non-list queries, types without a vector
  field, and documents combining `nearest` with `limit` or `orderBy` (`k` is the
  page size).

### Choosing among several vector fields

A type may declare more than one — a text embedding and an image embedding on the
same row — and `nearest.field` names which to search:

```graphql
{ docs(nearest: { vector: $q, k: 10, field: "imageEmbedding" }) { id } }
```

`field` is optional on a type with exactly one vector field, and **required** on a
type with several: the omission is ambiguous rather than convenient, and the refusal
names the candidates. Answering it by declaration order would search one embedding
space and report the result as another — a query that succeeds and means something
else.

The selected field's own `dimensions` and `distance_metric` apply, so the dimension
check is against the field being searched.

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
- `hamming_distance` / `jaccard_distance` take a **bit string** operand and
  apply to `BitVector` fields — see below.
- Performance: ANN indexes accelerate `ORDER BY … LIMIT` (the `nearest` shape),
  not bare threshold predicates — expect a scan on large tables, or combine
  with `nearest`. How much of a scan is measured below.

## Index eligibility: what a filter costs an ANN search

`benches/vector_filtered_ann.sql` measures both shapes against 100 000 documents
of 384 dimensions with an HNSW index. Run it against any pgvector 0.8+ database:

```bash
psql "$DATABASE_URL" -f benches/vector_filtered_ann.sql
```

Measured on pgvector 0.8.6 / PostgreSQL 16.14, `k = 10`, filter uncorrelated
with the embedding (a tenant or status predicate, not a topic):

| rows matching the filter | `hnsw.iterative_scan` | rows returned | recall | median |
|---|---|---|---|---|
| all (no filter) | any            | 10 | 1.00 | 0.11 ms |
| 50%             | any            | 10 | 1.00 | 0.24 ms |
| 5%              | `off` (default)| **3** | **0.30** | 0.43 ms |
| 5%              | `relaxed_order`| 10 | 1.00 | 1.9 ms |
| 1% and below    | `off` (default)| **2** | **0.20** | 0.43 ms |
| 1% and below    | `relaxed_order`| 10 | 1.00 | 3.1 ms |

**The failure mode is not slowness.** With pgvector's default
`hnsw.iterative_scan = off`, the index scan hands up a bounded candidate list and
the filter is applied to it; once the filter is selective the list is exhausted
before ten survivors exist, and the query returns two rows and succeeds. A client
asking for the ten nearest gets two, with nothing to distinguish that from "only
two matched".

**FraiseQL sets this GUC** (#1116). Every connection its pools open carries

```
-c hnsw.iterative_scan=strict_order -c ivfflat.iterative_scan=relaxed_order
```

in the PostgreSQL startup packet, the same place the per-tenant `search_path`
rides — a property of how connections are *made*, so it reaches every connection
the pool opens later and survives `DISCARD ALL`. It costs no extra round trip, and
it is inert on a database without pgvector: a two-part GUC name is accepted as a
placeholder when no extension has claimed it.

The defaults, and why:

| setting | default | values |
|---|---|---|
| `vector_hnsw_iterative_scan` | `strict_order` | `off`, `relaxed_order`, `strict_order` |
| `vector_ivfflat_iterative_scan` | `relaxed_order` | `off`, `relaxed_order` |
| `vector_hnsw_ef_search` | *unset* (pgvector's 40) | any positive integer |

`strict_order` for HNSW because `nearest` is documented to return rows
most-similar-first, and a plan whose ordering comes from the index scan can only
honour that if the scan is ordered — `relaxed_order` would trade the silent
under-return for a quieter version of the same problem. The benchmark above found
the two indistinguishable in recall *and* latency, so the guarantee is the only
thing separating them; an operator whose corpus disagrees can say so. IVFFlat has
no `strict_order` — pgvector defines only `off` and `relaxed_order` there, which
is why the two settings have different value sets rather than one shared one.

All three are **server** configuration — the same flat-key file as
`pool_max_size` and `read_replica_urls`, read by `ServerConfig`, not the
authoring `fraiseql.toml`:

```toml
# Leave pgvector's own defaults in place.
vector_hnsw_iterative_scan = "off"
vector_ivfflat_iterative_scan = "off"

# Or widen the candidate list, when the section below says that is the knob
# your corpus needs.
vector_hnsw_ef_search = 200
```

`off` writes **nothing** into the startup packet rather than writing
`-c hnsw.iterative_scan=off`, so a database-level setting still applies:

```sql
ALTER DATABASE app SET hnsw.iterative_scan = relaxed_order;  -- still honoured
```

⚠ A bad value is not an error. pgvector defines these GUCs when its library loads,
after the connection is already established, so an unrecognised value produces
`WARNING: invalid value for parameter "hnsw.iterative_scan"` and falls back to
`off` — the very failure the setting exists to prevent. That is why the
configuration is an enum per index rather than a string.

### ⚠ `iterative_scan` alone does not guarantee `k` rows

Turning it on is a large improvement and it is not a complete fix. Measured on this
repository's rig — 20 000 rows × 64 dimensions of uniform-random vectors, a filter
matching 1%, `k = 10`, three **independently generated** datasets:

| configuration | rows returned |
|---|---|
| `iterative_scan=off` (pgvector's default) | 2, 3, 4 |
| `iterative_scan=strict_order` | 10, 5, 4 |
| …plus `max_scan_tuples` raised 100× | 10, 5, 4 — **no effect** |
| …plus `ef_search=1000` instead | **10, 10, 10** |
| `iterative_scan=off` **and** `ef_search=1000` | **10, 10, 10** |

`hnsw.ef_search` (default 40) is the size of the candidate list the graph search keeps.
When a filter admits 1% of rows, a 40-candidate list rarely contains ten survivors, and
continuing the scan does not help a search that is not looking *wider*. On that corpus
`ef_search` is the binding constraint and `iterative_scan` is not needed at all; on the
100 000 × 384 corpus in `benches/vector_filtered_ann.sql` the opposite holds and
`iterative_scan` alone reaches recall 1.00.

**Which knob binds is a property of your data.** Uniform-random high-dimensional vectors
are the adversarial case — everything is nearly equidistant, so the graph is close to
arbitrary — and real embeddings cluster. The practical procedure:

1. Leave `vector_hnsw_iterative_scan` on. It is free on unfiltered searches.
2. If filtered searches still return fewer than `k`, raise `vector_hnsw_ef_search`
   (try 200, then 1000) and measure. Do **not** reach for `max_scan_tuples`; it changed
   nothing in every case measured here.
3. `ef_search` is left unset by default because it costs latency on *every* search,
   filtered or not, and only you know your corpus.

⚠ Whatever you set, there is still **no signal** when a search gives up early: a client
that asked for ten nearest and received three sees a successful response carrying three
rows, indistinguishable from "only three matched". That gap is #1314.

Threshold predicates are a different story: a distance *range* is not what an ANN
index answers, so no setting makes one index-eligible. Both forms read every row.
What differed was where the vector came from:

| threshold predicate | median |
|---|---|
| against the native `vector(N)` column — what runs now | 22 ms |
| through the JSONB payload (`(data->>'embedding')::vector`) — before #1117 | **2667 ms** |

That 122× was the per-row text parse: every row's embedding rendered to text in
the view, extracted from the JSONB and parsed back into a `vector(N)`. The WHERE
generator used to resolve a vector field like every other filter, so a threshold
predicate paid it on the same view that already exposed the column `nearest`
orders by. It now reads the column, and the JSONB row above is kept only to say
what the storage contract is protecting you from.

A threshold predicate still reads every row, so on a large table prefer `nearest`
with a `k` — that shape is the index-eligible one.

## The distance in the response

A `Float` field declaring `vector_distance` carries the distance the search
ordered by:

```toml
[types.Doc.fields.similarity]
type = "Float"
vector_distance = "embedding"
```

```graphql
{ docs(nearest: { vector: $q, k: 10 }) { id title similarity } }
```

- The value is projected from **the same expression** the `ORDER BY` was built
  from, so the number a row reports and the position it occupies cannot be
  computed two different ways. Override the metric and the reported number
  follows it — including pgvector's negated inner product, where a closer row
  reports a *smaller* (more negative) value.
- It is not a stored column. Nothing needs to be added to the view, and nothing
  writes it.
- Selecting it on a query that ran no `nearest` search, or one that searched a
  different vector field, is **refused**. A null would be indistinguishable from
  a row whose distance is genuinely unknown, on a response that otherwise looks
  like it succeeded.
- The declaration is checked at compile time: the named field must exist on the
  same type and be a vector field, and the declaring field must be a `Float`.

## Half-precision and sparse vectors

pgvector stores a float vector three ways, and FraiseQL declares each (#959):

| `type` | Column | GraphQL | `nearest.vector` |
|--------|--------|---------|------------------|
| `"Vector"` | `vector(N)` | `[Float!]!` | `[0.1, 0.2, …]` |
| `"HalfVector"` | `halfvec(N)` | `[Float!]!` | `[0.1, 0.2, …]` |
| `"SparseVector"` | `sparsevec(N)` | `String` | `"{1:0.5,7:0.25}/1000"` |

All three take `cosine` / `l2` / `inner_product`, and the operator class follows
the column type — `halfvec_cosine_ops`, `sparsevec_l2_ops` — so `--emit-ddl`
emits the right one without being told.

- **`HalfVector`** halves the storage and the HNSW index memory, at `f16`
  precision (about three decimal digits per component). The query surface is
  unchanged: the same `[Float!]` operand, and PostgreSQL resolves the literal to
  the column's type.
- **`SparseVector`** takes pgvector's own text form, 1-based
  `{index:value,…}/dimensions`. A dense `[Float!]` operand is refused: a sparse
  vector exists so that a 30-thousand-dimension bag of terms is never written out
  in full, and accepting the dense form would give that back. Indices outside
  `1..=dimensions`, and a dimension count that disagrees with the declaration,
  are named errors.
- `index_type = "ivf_flat"` on a `SparseVector` is a compile error: pgvector 0.8
  ships no `sparsevec_*` class for ivfflat.
- The threshold WHERE operators take the same operand shapes — a string operand
  filters as a sparse vector, an array as a dense one.

## Binary (bit) vectors

pgvector's `hamming_distance` (`<~>`) and `jaccard_distance` (`<%>`) are defined
over `bit` values, not `vector` ones. Declare the field as `BitVector` (#959):

```toml
[types.Doc.fields.fingerprint]
type = "BitVector"
vector = { dimensions = 768, index_type = "hnsw", distance_metric = "hamming" }
```

- `dimensions` counts **bits**. `--emit-ddl` produces a `bit(768)` column, the
  `vector` extension (the type is PostgreSQL's, the operators are pgvector's) and
  an index with the `bit_hamming_ops` / `bit_jaccard_ops` operator class.
- The metric must match the field type: `hamming` / `jaccard` on `BitVector`,
  `cosine` / `l2` / `inner_product` on `Vector`. The cross pairing is a compile
  error — pgvector has no such operator.
- `index_type = "ivf_flat"` with `jaccard` is a compile error too: pgvector 0.8
  ships `bit_jaccard_ops` for `hnsw` only.
- In GraphQL the field is a `String` — a run of `0`/`1` characters, which is what
  `bit(N)`'s text form and `binary_quantize(embedding)::bit(768)` produce.

Storage follows the same contract as float vectors: a native `bit(N)` column for
`nearest`, and `fingerprint::text` inside `data` for threshold filtering.

Searching:

```graphql
{
  docs(nearest: { vector: "11110000", k: 10, metric: "jaccard" }) { id }
  hits: docs(where: { fingerprint: { hamming_distance: { vector: "11110000", threshold: 3 } } }) { id }
}
```

- `nearest.vector` is a string here, not an array; the wrong shape is refused by
  name.
- Its length must equal the declared `dimensions`. This check is load-bearing:
  casting text to `bit(N)` pads a short value on the right and truncates a long
  one, both silently, so an unchecked operand searches a different fingerprint
  and answers confidently.
- Both distances are "smaller is closer": hamming counts differing bits, jaccard
  is `1 - |intersection| / |union|` over set bits, so a sparse near-match beats a
  dense one under jaccard and loses to it under hamming.
- Every emitted cast is `varbit`, never `bit`: `'1011'::bit` is `bit(1)`.
