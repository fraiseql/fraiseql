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

- `10.5` for an `Int`;
- `"abc"` for a `Float`;
- the string `"true"` for a `Boolean`;
- `"not-a-date"` for a `Date`.

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

- `neq`, `nin`, `nlike` and `nilike` do not return rows whose key is null or absent.
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
| `in` | `v` equals some element of `o` | a list of scalars | `[]` is false for every row |
| `nin` | `v` equals no element of `o` | as `in` | unknown on null or absent; see decision D1 for `[]` and D2 for `null` elements |

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

The operand must be a JSON array of scalars; anything else is refused. The field holds a
JSON array of scalars, per the view's contract. Elements compare as JSON values:

- `1` equals `1.0`;
- `"1"` does not equal `1`.

Duplicates do not count. Object and array elements are outside this contract (decision
D7).

| Operator | True when | Empty arrays |
|---|---|---|
| `array_contains` | every element of `o` is an element of `v` | `o = []` is true for every array |
| `array_contained_by` | every element of `v` is an element of `o` | `v = []` is true for every operand |
| `array_overlaps` | some element of `o` is an element of `v` | false when either side is `[]` |

A null or absent key is unknown.

## Decisions for review

These rows record today's behaviour where it is a choice, with a recommendation. When one
is ruled, the table above states the ruling and the row is removed.

| # | Question | Today | Recommendation |
|---|---|---|---|
| D1 | `nin: []` | true for **every** row, including null and absent, while `nin: [x]` is unknown on them | make it unknown on null and absent like any other `nin`, so adding an element never adds rows |
| D2 | a `null` element in `in` / `nin` | follows SQL: `in: [a, null]` is true only on `a`, and `nin: [a, null]` is never true | refuse a `null` element, and point to `isnull`: no client means "never" |
| D3 | a string operand for a declared `Int` or `Float` (`"5"`) | accepted, and read as the number | refuse it, as GraphQL input coercion does for `Int` and `Float` |
| D4 | the string operators on a non-string value | match its JSON text (`contains: "5"` matches a stored `15`) | keep, stated as above; a declared non-text field could refuse them instead |
| D5 | array operators on a declared `String` field holding an array | allowed (the declared type is not consulted) | keep until fields can declare a list type |
| D7 | object or array elements in the array operators | PostgreSQL `jsonb` containment: `[{"a":1,"b":2}]` contains `[{"a":1}]`, and `[[1,2]]` contains `[[1]]` | refuse non-scalar elements in the operand until a field can declare a list of objects |
| D6 | the text operators under the database's own collation | follow it | keep: this page states the builtin `C.UTF-8` behaviour, and deployments that choose another collation get its order and folding |

## Not covered

These operators are not modelled by O3:

- `regex` and its forms: PostgreSQL's regular-expression dialect is not modelled. O1 and
  O2 still check them.
- The length operators.
- Full-text search, network, ltree and vector operators.
- The accent-insensitive operators: see
  [accent-insensitive-filters.md](features/accent-insensitive-filters.md).
