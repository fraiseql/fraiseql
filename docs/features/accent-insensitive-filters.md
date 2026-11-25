# Accent-insensitive filters (`[accent_insensitive]`)

`icontains` and the other `i*` operators fold case only: `creme` does not find `Crème
brûlée`. A deployment that declares `[accent_insensitive]` gets a second family that folds
accents as well, through PostgreSQL's `unaccent` extension.

```toml
# fraiseql.toml
[accent_insensitive]
function = "fraiseql.unaccent"   # optional; this is the default
```

```graphql
{ products(where: { name: { aicontains: "creme" } }) { name } }
```

finds `Crème brûlée`, `CREME fraiche` and `crème_50%`.

## Operators

Each is accent- and case-insensitive. It is published on every string filter
(`StringFilter`, and the filters of custom scalars stored as text), and only when the schema
declares `[accent_insensitive]`. Without the section the operators are neither published
nor accepted: a filter naming one is refused, pointing at the section.

| Operator | Case-insensitive counterpart | SQL, with `F` the declared function |
|---|---|---|
| `aicontains` | `icontains` | `F(value) ILIKE '%' \|\| escaped(F($1)) \|\| '%'` |
| `aistartswith` | `istartswith` | `F(value) ILIKE escaped(F($1)) \|\| '%'` |
| `aiendswith` | `iendswith` | `F(value) ILIKE '%' \|\| escaped(F($1))` |
| `aieq` | (none) | `lower(F(value)) = lower(F($1))` |
| `ailike` | `ilike` | `F(value) ILIKE F($1)` |
| `nailike` | `nilike` | `F(value) NOT ILIKE F($1)` |
| `airegex` | `iregex` | `F(value) ~* F($1)` |
| `nairegex` | `niregex` | `F(value) !~* F($1)` |

Both sides are folded: `?name[aicontains]=CRÈME` and `aicontains: "creme"` find the same
rows.

### The operand is folded too

- **`aicontains`, `aistartswith`, `aiendswith`** match their operand literally. Its `%`,
  `_` and `\` are escaped *after* the fold, because the dictionary folds the full-width
  `％`, `＿` and `＼` into `%`, `_` and `\`: `aicontains: "％"` finds the rows that hold a
  percent sign, either one, not every row.
- **`ailike`, `nailike`** take a `LIKE` pattern, and the pattern is folded as written, its
  wildcards included: `cr_me%` and `%BRÛLÉE` behave as written in plain ASCII, and `\_`
  still matches a literal underscore. A full-width `％` or `＿` in the pattern folds into a
  wildcard.
- **`airegex`, `nairegex`** take a POSIX pattern, folded before PostgreSQL compiles it. A
  character class is folded with the rest: `[éè]` becomes `[ee]`, so `^cr[è]me` also
  matches `creme`.

### The limits on a regex pattern

A regex pattern is held to the same limits as `regex` and `iregex`: at most 1000 bytes, and
no quantified group inside a quantified group (`(a+)+`). The fold could change a pattern
after those checks: the dictionary turns `（a＋）＋` into `(a+)+`, `⑴` into `(1)` and `ˆ`
into `^`. So beyond ASCII, an `airegex` or `nairegex` pattern may hold only letters (other
than modifier letters) and combining marks, and any other character is refused, naming it.
None of those letters folds into regex syntax or into a longer text, so the limits checked
on the pattern you wrote hold for the pattern PostgreSQL compiles.

## The function

PostgreSQL's `unaccent()` is `STABLE`: it looks its dictionary up when it is called, so no
index expression can use it. The filters call an `IMMUTABLE` wrapper that names the
dictionary, which is what makes declaring it `IMMUTABLE` sound:

```sql
CREATE SCHEMA IF NOT EXISTS "fraiseql";
CREATE EXTENSION IF NOT EXISTS unaccent WITH SCHEMA "public";
CREATE OR REPLACE FUNCTION "fraiseql"."unaccent"(text) RETURNS text
    LANGUAGE sql IMMUTABLE PARALLEL SAFE STRICT
    RETURN "public".unaccent('"public".unaccent'::regdictionary, $1);
```

The server never creates it:

- `fraiseql compile --emit-ddl <dir>` writes these statements to `<dir>/accent-insensitive.sql`
  (see [the `--emit-ddl` artifact](../operations/emit-ddl.md)).
- `fraiseql compile --database <url>` refuses a database that has no `unaccent` extension,
  no wrapper, or a wrapper that is not `IMMUTABLE` or does not return `text`, printing the
  statement that creates it. When the extension lives in another schema than `public`, the
  printed wrapper reads that schema's dictionary.

`function` names the wrapper. It must be schema-qualified and lower case (`schema.name`,
each part a letter or underscore followed by letters, digits or underscores): an unqualified
name would resolve through each session's `search_path`. `fraiseql compile` refuses any other
shape, and the server refuses it again when it loads `schema.compiled.json`.

The wrapper folds through the `unaccent` dictionary it names. A deployment that edits that
dictionary's rules changes what every filter matches, and must reindex what it indexed.

## Indexes

The filters fold the value they read, so an index serves them when it indexes the same
expression. For a view that passes a table's `data` column through:

```sql
CREATE EXTENSION IF NOT EXISTS pg_trgm;
-- aicontains, aistartswith, aiendswith, ailike
CREATE INDEX ON tv_product USING gin (fraiseql.unaccent(data->>'name') gin_trgm_ops);
-- aieq
CREATE INDEX ON tv_product (lower(fraiseql.unaccent(data->>'name')));
```

PostgreSQL 18 uses both under the generic plan the server prepares, and through a view
that selects `data` from the table. It cannot use them through a view that builds `data`
with `jsonb_build_object(...)`: index the column the view reads instead.

## Where they run

- **GraphQL and REST.** REST accepts `aieq`, `ailike`, `aicontains`, `aistartswith` and
  `aiendswith` as bracket operators (`?name[aicontains]=creme`), and all eight in
  `?filter=`. Its OpenAPI document lists them only where the schema declares the section.
- **Localized fields.** The label the request locale reads is folded, as `icontains`
  compares it.
- **Relations.** A filter through a relation reads the related value through that
  relation's row guard first, as every other operator does.
- **Not on fact tables, nor on the wire backend.** A fact-table aggregate filter and the
  `fraiseql-wire` backend refuse the operators by name, rather than run them as plain
  comparisons.

Changing `[accent_insensitive]` needs a restart: a hot reload that changes it is refused,
because the REST surface reads it when it is mounted.
