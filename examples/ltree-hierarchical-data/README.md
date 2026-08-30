# LTREE hierarchical data

Two examples using PostgreSQL's `ltree` type with FraiseQL, where the hierarchy
question is a WHERE filter rather than a recursive CTE or a closure table.

| Example | Tree | What the path means |
|---|---|---|
| [`organization-chart/`](organization-chart) | management chain | every label is an employee; a label's parent is their manager |
| [`product-catalog/`](product-catalog) | category taxonomy | every label is a category; products sit at the leaves |

The distinction matters. In `organization-chart` the entities *are* the nodes, so
`ancestorOf` returns a person's whole management chain. In `product-catalog` the
nodes are categories and the entities hang off them, so `descendantOf` is the
useful operator and `ancestorOf` only ever matches the row itself.

## Running one

```bash
cd organization-chart

# 1. Schema and sample data (needs the ltree extension; the file creates it)
createdb orgchart
psql -d orgchart -v ON_ERROR_STOP=1 -f setup.sql

# 2. Author the GraphQL schema (writes schema.json)
python3 schema.py

# 3. Compile it
fraiseql compile schema.json -o schema.compiled.json

# 4. Run one of queries.graphql against the database
DATABASE_URL=postgresql://localhost/orgchart \
  fraiseql query -s schema.compiled.json \
  '{ employees(where: {orgPath: {descendantOf: "acme.alice_johnson.bob_smith"}}) { name title } }'
```

`product-catalog` is the same four steps.

## The operators

Declared in each example's `schema.py`, on a filter input for the path field:

| GraphQL | SQL | Meaning |
|---|---|---|
| `eq` | `=` | exactly this path |
| `descendantOf` | `<@` | at or below this path, any depth |
| `ancestorOf` | `@>` | at or above this path |
| `depthEq` | `nlevel() =` | exactly this many labels |

They are **declared, not derived**. FraiseQL does not auto-derive the ltree
operator family onto a field: a declared field type cannot say whether the column
behind it is really an `ltree`, and deriving a filter advertises an operator
(#869). Writing the filter input is the author asserting that the column is one.

The engine also has `descendantOfId` / `ancestorOfId`, which take a UUID and
resolve it to a path through a `[hierarchies.<name>]` section in `fraiseql.toml`.
Neither example configures one, so neither declares those operators.

Note that every WHERE field is `{operator: value}`. A bare `department:
"Engineering"` is refused by the parser, which is why the ordinary columns here
get a `StringFilter` too.

## The read side

Both examples follow the Trinity pattern: `tb_*` holds the rows, `v_*` exposes
`pk_*`, `id` and a JSONB `data` column, and the runtime reads `data`. The ltree
path is a plain string inside `data`; the generated WHERE casts it back with
`(data->>'…')::ltree` before applying an operator.
