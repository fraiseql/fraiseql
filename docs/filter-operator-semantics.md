# Filter operator semantics

What a `where` filter means, operator by operator, over the JSON document a view serves.
This page is a contract: the SQL oracle's reference evaluator (O3 in
`crates/fraiseql-db/tests/sql_oracle_pg.rs`) is written from it and from nothing else, and
a generator that answers differently is wrong, not the table. The table changes by review,
as `graphql-conformance.md` does.

It covers 23 operators. The rest are checked only for being accepted by PostgreSQL (O1) and
for the two generators agreeing (O2) until a row is added here.

## Reading a field

A leaf `{ field: { op: operand } }` reads the document key `field` of each row (a nested
path reads nested keys). Each row's key is in one of three states:

| State | Example document | Read as |
|---|---|---|
| **absent** | `{}` | SQL `NULL` |
| **null** | `{"name": null}` | SQL `NULL` |
| **present** | `{"name": "alpha"}` | its value |

*absent* and *null* are indistinguishable to every operator on this page.

A present value is read in one of two ways:

- **As text** for the string operators: a JSON string is its contents, and any other JSON
  value is its JSON text (`5` reads as `"5"`, `true` as `"true"`).
- **As the comparison type** for the comparison operators: see the next section.

## The comparison type

`eq`, `neq`, `gt`, `gte`, `lt`, `lte`, `in` and `nin` compare in one type, chosen once per
leaf:

1. **The field's declared type**, when the schema declares one:

   | Declared | Compared as | Order |
   |---|---|---|
   | `String`, `ID` | text | code point (see Collation) |
   | `Int` | 64-bit integer | numeric |
   | `Float`, `Decimal` | exact decimal | numeric (`19.90 = 19.9`) |
   | `Boolean` | boolean | `false < true` |
   | `Date` | calendar date | chronological |
   | `DateTime` | instant | chronological (`10:00+02:00 = 08:00Z`) |

2. **Otherwise, the operand's JSON shape:**
   - a number compares as an exact decimal;
   - a boolean as a boolean;
   - anything else as text.

   An `in` / `nin` list whose elements all share one shape uses that shape. A list mixing
   shapes compares as text, so `n: { in: [5, "x"] }` matches a stored `5`.

**The operand must be readable as the comparison type.** If it is not, the filter is
refused before any SQL is sent, with a validation error naming the argument:

- an `Int` takes a JSON integer within 64 bits: `10.5`, `1e300` and the string `"5"` are
  refused;
- a `Float` takes a JSON number: `"abc"` and the string `"5"` are refused;
- a `Boolean` takes a JSON boolean: the string `"true"` is refused;
- a `Date` or `DateTime` takes a string in its scalar's format: `"not-a-date"` is refused;
- a text operand may not contain NUL.

**Stored values are assumed to conform to the declared type.** That is the view's
contract with the schema. The meaning of a row whose stored value does not conform is not
defined here, and the oracle's fixture holds none.

## Collation and case

- **Text order and text equality are by Unicode code point.**
  - `"Z" < "a"`, and `"é" > "f"`.
  - Precomposed `"é"` and `"e"` + combining acute are different strings.
- **The case-insensitive operators fold with Unicode simple case mapping.**
  - `"É"` matches `"é"`.
  - `"ß"` does not match `"SS"`: that is full case folding, which is not used.
- This is PostgreSQL's builtin `C.UTF-8` locale. The oracle creates its database with it,
  so its answers do not depend on the server's default locale.
- A deployment whose database uses another collation orders and folds text by that
  collation instead. Those differences are outside this contract.

## Null logic

A leaf over a *null* or *absent* key is **unknown** for every operator except `isnull` and
`is_not_null`. `and`, `or` and `not` combine true, false and unknown with SQL's
three-valued logic, and a row is returned only when the whole filter is **true**.

Consequences:

- `neq`, `nin`, `nlike` and `nilike` do not return rows whose key is null or absent. That
  holds for `nin: []` too: it is true on every present value and unknown on the rest, so
  adding an element to a `nin` list never adds a row.
- `not: { name: { eq: "a" } }` does not return them either.
- To include them, write `or: [{ name: { neq: "a" } }, { name: { isnull: true } }]`.

An empty `and: []` is true, and an empty `or: []` is false.

## Operators

`v` is the row's value, read as above; `o` is the operand. "Refused" means a validation
error before SQL.

### Comparison and membership

| Operator | True when | Operand | Notes |
|---|---|---|---|
| `eq` | `v = o` | a scalar of the comparison type | |
| `neq` | `v ≠ o` | as `eq` | unknown on null or absent |
| `gt` / `gte` / `lt` / `lte` | `v > o`, `v ≥ o`, `v < o`, `v ≤ o` in the type's order | as `eq` | text by code point |
| `in` | `v` equals some element of `o` | a list of scalars of the comparison type; a `null` element is refused (use `isnull`) | `[]` is false for every row |
| `nin` | `v` equals no element of `o` | as `in` | unknown on null or absent, `[]` included |

### Null tests

| Operator | True when | Operand |
|---|---|---|
| `isnull` | `o` is `true` or `null`: the key is null or absent. `o` is `false`: it is present. | a boolean or `null`; anything else is refused |
| `is_not_null` | the negation of `isnull` with the same operand | as `isnull` |

### Strings: literal match

The operand is literal text. `%`, `_` and `\` match themselves. The operand must be a
string, or the filter is refused.

| Operator | True when |
|---|---|
| `contains` | `o` occurs in `v` |
| `startswith` | `v` begins with `o` |
| `endswith` | `v` ends with `o` |
| `icontains`, `istartswith`, `iendswith` | the same, after case folding both sides |

An empty operand is true for every present value (every string contains `""`).

### Strings: patterns

The operand is a SQL `LIKE` pattern:

- `%` matches any run of characters, including an empty one;
- `_` matches exactly one character;
- `\` makes the next character literal.

A pattern ending in a lone `\` is refused. The operand must be a string.

| Operator | True when |
|---|---|
| `like` | the whole of `v` matches the pattern |
| `ilike` | the same, after case folding both sides |
| `nlike` / `nilike` | the negation of `like` / `ilike`; unknown on null or absent |

An empty pattern matches only the empty string.

### Arrays

The operand must be a JSON array whose elements are scalars: an object or array element
is refused, and so is any other operand. The field holds a JSON array of scalars, per the
view's contract; its declared type is not consulted, so a field declared `String` that
holds an array takes these operators. Elements compare as JSON values:

- `1` equals `1.0`;
- `"1"` does not equal `1`.

Duplicates do not count.

| Operator | True when | Empty arrays |
|---|---|---|
| `array_contains` | every element of `o` is an element of `v` | `o = []` is true for every array |
| `array_contained_by` | every element of `v` is an element of `o` | `v = []` is true for every operand |
| `array_overlaps` | some element of `o` is an element of `v` | false when either side is `[]` |

A null or absent key is unknown.

## Not covered

These operators are not modelled by O3:

- `regex` and its forms: PostgreSQL's regular-expression dialect is not modelled. O1 and
  O2 still check them.
- The length operators.
- Full-text search, network, ltree and vector operators.
- The accent-insensitive operators: see
  [accent-insensitive-filters.md](features/accent-insensitive-filters.md).
