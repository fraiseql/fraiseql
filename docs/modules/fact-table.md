# Fact Table Module

**Source file**: `crates/fraiseql-core/src/compiler/fact_table/` (split into `mod.rs`, `detector.rs`, `tests.rs`)

**Tests**: 30+ unit tests in `#[cfg(test)] mod tests` at the bottom of the file. Run with:

```bash
cargo nextest run -p fraiseql-core --lib compiler::fact_table
```

---

## Overview

The fact table pattern is FraiseQL's approach to analytics workloads. It is inspired by
dimensional modeling (Kimball-style data warehousing) but adapted for PostgreSQL JSONB.

The module provides:

1. **Auto-detection** — discovers `tf_*` tables by name convention and introspects their structure
2. **JSONB dimension extraction** — samples actual data to infer dimension key paths
3. **Query validation** — validates window function requests against discovered table metadata

---

## The Fact Table Pattern

### Naming: `tf_*` tables

Any table whose name starts with `tf_` (table fact) is treated as a fact table.
Detection: `name.starts_with("tf_") && name.len() > 3`.

Examples: `tf_sales`, `tf_events`, `tf_page_views_daily`

### Structure: Measures + JSONB Dimensions + Denormalized Filters

```sql
CREATE TABLE tf_sales (
    id           BIGSERIAL PRIMARY KEY,

    -- Measures: numeric SQL columns for fast GROUP BY aggregation
    revenue      DECIMAL(10,2) NOT NULL,
    quantity     INT NOT NULL,
    cost         DECIMAL(10,2) NOT NULL,

    -- Dimensions: JSONB for flexible, schemaless grouping
    dimensions   JSONB NOT NULL,
    -- e.g. {"category": "Electronics", "region": "APAC", "channel": "Online"}

    -- Denormalized filters: indexed SQL columns for fast WHERE
    customer_id  UUID NOT NULL,
    product_id   UUID NOT NULL,
    occurred_at  TIMESTAMPTZ NOT NULL
);

CREATE INDEX ON tf_sales(customer_id);
CREATE INDEX ON tf_sales(product_id);
CREATE INDEX ON tf_sales(occurred_at);
```

**Why this design?**

- **No joins at query time**: ETL denormalizes dimensions into JSONB at load time.
  Analytical queries aggregate over a single table, avoiding multi-way joins.
- **Measures in SQL columns**: `SUM(revenue)` uses native SQL aggregation — fast and
  index-friendly.
- **Dimensions in JSONB**: Flexible schema. Adding a new dimension (`"sub_region"`) requires
  no ALTER TABLE — just add the key in ETL.
- **Indexed filter columns**: `WHERE customer_id = $1 AND occurred_at >= $2` uses B-tree
  indexes, pushing filtering down before aggregation.

---

## Introspection Flow

```text
DatabaseIntrospector::list_fact_tables()
      ↓ finds all tables starting with "tf_"
For each table:
  DatabaseIntrospector::get_columns(table_name)
      ↓ returns Vec<(name, data_type, is_nullable)>
  Classify each column:
      numeric AND NOT ends with "_id" AND NOT named "id"  → Measure
      JSONB or JSON                                        → DimensionColumn
      numeric AND ends with "_id"                          → Filter (INT FK)
      any other type (text, uuid, ltree, …)               → Filter
      created_at / occurred_at AND indexed                 → Filter (time)
  DatabaseIntrospector::get_sample_jsonb(table_name, "dimensions")
      ↓ SELECT dimensions FROM table LIMIT 100
  extract_dimension_paths(sample_jsonb, "dimensions", db_type)
      ↓ returns Vec<DimensionPath>
FactTableMetadata { measures, dimensions, denormalized_filters }
```

The resulting `FactTableMetadata` is used by `WindowPlanner` to validate that dimension
and measure names in window function requests actually exist on the table.

---

## JSONB Dimension Extraction

### Sampling Strategy

Instead of requiring explicit dimension declarations, FraiseQL samples the JSONB column
to discover dimension key paths:

```rust,ignore
pub fn extract_dimension_paths(
    sample: &serde_json::Value,
    column_name: &str,
    db_type: DatabaseType,
) -> Vec<DimensionPath>
```

The sampler walks the JSON structure recursively with a **max depth of 3** to avoid
infinite recursion on circular or deeply nested structures.

For each key found, it generates a PostgreSQL extraction expression, e.g.
`dimensions->>'category'`, or `dimensions->'machine'->>'model'` for a nested key (named
`machine_model`).

### The `json_path` contract

A declared dimension path is read where its `json_path` says, by `groupBy` and by `where`
(aggregate and window queries alike). So `json_path` has one accepted shape: the dimensions
column, any number of `->'key'` steps, and one final `->>'key'`, each key a single-quoted
literal with `'` written `''`. `fraiseql compile` and the server's schema load refuse any
other shape (an expression such as `(data->>'amount')::int`, another column, trailing text),
naming the path. A numeric dimension is declared with its `data_type`, not with a cast.

### Data type inference

The sampler infers types from the observed JSON value type:

| JSON value | Inferred type |
|-----------|---------------|
| `"Electronics"` | `string` |
| `42` (integer) | `integer` |
| `3.14` | `float` |
| `true`/`false` | `boolean` |
| `[...]` | `array` |
| `{...}` | `object` (nested) |

### Limitations

- **Single-sample extraction**: Path discovery uses one sample row. Heterogeneous JSONB
  (some rows have extra keys) means not all paths will be discovered from one sample.
  If a dimension is missing from the sample, it will not appear in the metadata.
- **Arrays are opaque**: Array-type dimensions are listed but not expanded. Array element
  paths must be declared explicitly if needed.
- **Max depth 3**: Deeply nested structures are truncated. For `{a: {b: {c: {d: ...}}}}`,
  paths up to `a.b.c` are discovered; `a.b.c.d` is not.

---

## Tree Paths (`ltree`)

A fact table whose rows sit in a tree (an org chart, a location, a taxonomy) keeps each
row's path in an `ltree` column, declared as a denormalized filter with
`"sql_type": "LTREE"`. Introspection classifies such a column as an `Ltree` filter.

```sql
CREATE TABLE tf_headcount (
    salary    NUMERIC NOT NULL,
    org_path  LTREE NOT NULL,          -- e.g. 'acme.engineering.platform'
    data      JSONB NOT NULL DEFAULT '{}'
);
CREATE INDEX ON tf_headcount USING gist (org_path);
```

**Filter** on the path with the ltree operators: `org_path_descendant_of`,
`org_path_ancestor_of`, `org_path_matches_lquery`, `org_path_depth_eq` and the rest.

**Roll up** to a level of the tree, or group by depth:

```json
{ "groupBy": { "org_path_level": 2 },     "aggregates": [{ "salary_sum": true }] }
{ "groupBy": { "org_path_depth": true },  "aggregates": [{ "salary_sum": true }] }
```

`org_path_level: n` groups by `subpath(org_path, 0, n)`: each path cut to its first `n`
labels, with a shorter path kept whole, answering as `org_path_level`. `org_path_depth: true`
groups by `nlevel(org_path)` and answers as `org_path_depth`. Both need the column to be an
`LTREE` denormalized filter.

**Filter by node id.** `org_path_descendant_of_id` and `org_path_ancestor_of_id` take the
`id` of a node and compare against that node's path. The column names the hierarchy the
node lives in, declared in `fraiseql.toml`: the TOML schema, or the project config beside
an SDK's `schema.json`. An SDK emits the column's `hierarchy` name and never the
definition:

```toml
# fraiseql.toml
[hierarchies.org]
table = "tb_org_unit"      # holds id (UUID) + path
path_column = "path"
```

```json
"denormalized_filters": [
  {"name": "org_path", "sql_type": "LTREE", "indexed": true, "hierarchy": "org"}
]
```

A schema whose filter names an undeclared hierarchy, or links one to a column that is not
`LTREE`, is refused when the server loads it.

---

## Temporal buckets

`groupBy: {occurred_at_month: true}` (keyed `occurred_at_month`) groups by a bucket of a
denormalized time column: `second`, `minute`, `hour`, `day`, `week` (starting Monday),
`half_month` (starting on the 1st and on the 16th), `month`, `quarter`, `semester`
(starting on 1 January and on 1 July) or `year`. The key follows the column's declared
`sql_type`:

| Column | Bucket key | Example (`month`) |
|--------|-----------|-------------------|
| `date` | the date the bucket starts on, in no time zone | `"2026-07-01"` |
| `timestamp` (without time zone) | the date and time of day the bucket starts at, in no time zone | `"2026-07-01T00:00:00"` |
| `timestamptz` | the instant the bucket starts at, in the session's `TimeZone`, rendered in UTC | `"2026-06-30T15:00:00+00:00"` under `Asia/Tokyo` |

A `date` bucket does not depend on the session time zone. A sub-day grain (`hour`, `minute`,
`second`) of a `date` column is the date itself. A `timestamptz` half-month or semester
starts at midnight in the session's `TimeZone`, as a month does, so its half is decided by
the local date: `2026-03-16T00:30:00+01:00` is in the half starting on 16 March.

A bucket key is published as `Date` for a `date` column, as `LocalDateTime` for a
`timestamp` column without a time zone, and as `DateTime` for a `timestamptz` column, and
answers with the string above. The same holds for the column itself as a group key or a
`where` operand.

`DateTime` is an instant and always carries an offset. `LocalDateTime` is a date and a time
of day in no zone (`YYYY-MM-DDTHH:MM:SS`, optional fractional seconds, never an offset), as
the [`LocalDateTime` scalar](https://scalars.graphql.org/chillicream/local-date-time)
specifies. A `where` on a `timestamp` column compares local date-times, whatever the
session's `TimeZone`. Its operand is refused, naming the key, when it carries an offset or a
`Z`, which the `::timestamp` cast would drop, or is not a local date-time at all; so is a
`LocalDateTime` argument or variable, and a view field's `LocalDateTime` operand. An operand
on a `timestamptz`, `date` or `time` column is held to the `DateTime`, `Date` or `Time`
format the same way ([Temporal values](../features/temporal-values.md)). Declare the column's real type: `TIMESTAMPTZ` (or
`timestamp with time zone`) for an instant, `TIMESTAMP` (or `timestamp without time zone`)
for a local date-time; a `timestamptz` column declared `TIMESTAMP` is read as local.

A view that serves a `DateTime` field writes it from a `timestamptz` column:
`to_jsonb(timestamp)` writes no offset, which `DateTime` does not allow, and the engine
cannot see the column's type to tell.

### A rolled-up table: `grain`

A table that holds one row per period, dated on the period's first day, cannot answer for
part of a period. Its row dated 1 January holds all of January, so over 10 January to
20 March by month it drops January and counts March whole; and a week of it does not exist.
Declare the period its rows stand for, and such a request is refused instead:

```json
"grain": {"column": "day", "bucket": "month"}
```

`column` is one of the table's `date` denormalized filters, and `bucket` is `day`, `week`,
`half_month`, `month`, `quarter`, `semester` or `year`. A `timestamp` or `timestamptz`
column, a column that is not a filter, a column a calendar dimension reads, and an unknown
bucket or key are refused when the schema compiles.

On such a table, an aggregate or a window refuses:

- a bound on `column` inside a period. `_gte` and `_lt` take the day a period starts on, and
  `_gt` and `_lte` the day one ends on: under `month`, `day_gte: 2026-02-01`,
  `day_gt: 2026-01-31`, `day_lt: 2026-04-01` and `day_lte: 2026-03-31` are answered, and
  `day_gte: 2026-01-10` is refused, naming `2026-01-01` and `2026-02-01`. Any other
  operator on the column (`_eq`, `_neq`, `_in`, …) reads part of a period; `_isnull` reads
  none and is answered.
- a `groupBy` bucket of `column` a row does not fall wholly inside: under `month`, `day`,
  `week` and `half_month` are refused, and `month`, `quarter`, `semester` and `year` are
  answered. A week straddles months and years, so a `week` grain answers only `week`.

What is answered is answered as the finer rows would answer it, when each row holds its
period's sum (and, for a semi-additive `last`, its closing value). A `day` grain refuses
nothing. A table without a `grain` is read as before: a row stands for its own date.

### Linked grains: `edges_from`

A rolled-up table can name a finer table over the same rows, which answers what its own rows
would split:

```json
"grain": {"column": "day", "bucket": "month", "edges_from": "tf_sales_day"}
```

A range bound inside a month is then answered, not refused: the whole months inside the
range are read from the month table, the days around them from `tf_sales_day`, each under
the request's filters and grouping, and the two added per group. From 10 January to
20 May, February to April are read as month rows and 10–31 January and 1–20 May as day
rows. `having`, `orderBy`, `limit` and `offset` apply to the added groups, once. A range
with no whole month inside it is read from the finer table alone, and a range on month
boundaries from the month table alone, as before.

Only a sum of an additive measure recombines: a month's row holds the month's total, so a
`min`, `max`, `avg`, `count`, `stddev` or `variance` over month rows and day rows together
is neither table's answer. Over a range that is read from both tables, any other aggregate,
in `aggregates` or `having`, is refused, as is a semi-additive measure; bound the range on
month boundaries to read the months alone. Everything else the month table refuses for
itself it still refuses, in its own words, whatever the day table would accept: a `day_day`
`groupBy`, `day_eq`, a field it does not declare. A bound the finer table's rows would split
too (10 January, when the edges are half-months) is refused as an unlinked table refuses
it. The finer table is never named in an error of the coarse root. A window over a linked
table refuses a split range as before.

The two tables must be one table at two grains, or the schema is refused when it compiles
or loads:

- the finer table declares a `grain` on the same column, at a period that nests in the
  coarse one (`day` or `half_month` under `month`, `month` under `quarter`; not `week`
  under `month`), and links no edges of its own;
- the same measures (name, type, nullability, additivity; a measure's stored `column` may
  differ), denormalized filters, `dimensions`, `calendar_dimensions` and
  `native_dimension_mapping`;
- the same read gates: `type_name`, `row_scope` and `inject_params`. The edges are read
  under the coarse root's gates and filters, so a caller never reads finer rows under rules
  the coarse root does not hold it to.

What the compiler cannot check is that the coarse rows hold the finer rows' totals: the
answer is the finer table's answer when each coarse row is the sum of the finer rows in its
period, as a rollup refreshed from the finer table holds. A `float` measure's sum can differ
from the finer table's in its last digits, since the additions happen in another order; a
`bigint` or `numeric` sum is exact.

A linked aggregate is a parameterized read, and the query cache holds no parameterized
aggregate: it is read from the database every time, so a change to either table reaches the
next answer. The cover therefore needs no cache tags for the finer table; should
parameterized aggregates ever be cached, the entry must be invalidated by both tables.

---

## Calendar Dimensions (Performance Optimization)

Fact tables can include pre-computed temporal bucket columns for fast time-series aggregation:

```sql
CREATE TABLE tf_sales (
    ...
    date_info     JSONB NOT NULL,   -- {"date":"2024-03-15","week":11,"month":3,"quarter":1,"year":2024}
    month_info    JSONB NOT NULL,   -- {"month":3,"quarter":1,"year":2024}
    quarter_info  JSONB NOT NULL,   -- {"quarter":1,"year":2024}
    year_info     JSONB NOT NULL,   -- {"year":2024}
);
```

The introspector detects `*_info` columns with JSONB type and maps them to temporal
bucket levels:

| Column | Inferred buckets |
|--------|-----------------|
| `date_info` | date, week, month, quarter, year |
| `week_info` | week, month, quarter, year |
| `month_info` | month, quarter, year |
| `quarter_info` | quarter, year |
| `semester_info` | semester, year |
| `year_info` | year |

**Why pre-compute?** `DATE_TRUNC('month', occurred_at)` at query time applies a function
to every row. Indexes on `date_info->>'month'` are text comparisons — cheap and indexable.

---

## Measures that are not additive over time

A balance, a stock level or a headcount is **semi-additive**: summed across accounts it
means something, summed across days it does not. Such a measure declares how it aggregates
over time, and an aggregate over it is planned in two steps: the measure is first reduced
per entity and per bucket of the request's time grouping, then the requested function runs
across entities.

| `additivity` | Per entity, per bucket | Across entities |
|---|---|---|
| `additive` (the default) | the requested function, over the rows | the requested function |
| `semi_additive`, `using = last` | the last value known by the bucket's end | the requested function |
| `semi_additive`, `using = first` | the bucket's first value, else the value carried in | the requested function |
| `semi_additive`, `using = avg \| min \| max` | over the bucket's own rows | the requested function |
| `delta` | the bucket's last row minus its first | the requested function |
| `non_additive` | refused | refused |

`semi_additive` and `delta` name `over`, the denormalized `date` or `timestamp` column they
reduce over, and `entity`, the denormalized columns that identify what a value belongs to.
A declaration naming a column that is not there, or an `over` that is not a time column,
does not compile.

```typescript
SchemaRegistry.registerFactTable("tf_account_day", [
  { name: "closing_balance", sql_type: "numeric", nullable: false,
    additivity: { kind: "semi_additive", over: "day", using: "last", entity: ["account_id"] } },
  { name: "deposits", sql_type: "numeric", nullable: false },
], { name: "data", paths: [] }, [
  { name: "account_id", sql_type: "bigint", indexed: true },
  { name: "day", sql_type: "date", indexed: true },
]);
```

```go
fraiseql.NewFactTable("data").TableName("tf_account_day").
    SemiAdditiveMeasure("closing_balance", "numeric", false, "day", fraiseql.ReduceLast, "account_id").
    Measure("deposits", "numeric", false). /* filters … */ Register()
```

### Carried forward: `last` and `first`

A table that records a balance only on the days it changed has no row for most
(account, day) pairs. `last` carries each entity's last known value into every later
bucket, up to the end of the requested range: an account with a row in November holds
that value through March. So `closing_balance_sum` grouped by `day_month` sums each
account's balance as it stood at the end of each month, whether or not the account moved.

- **Closure is data.** A closed account carries its last value forward like any other.
  Record its closing row (a balance of 0) to end it; absence of rows is not read as closure.
- **The range** is the request's bounds on `over` (`eq`, `gt`, `gte`, `lt`, `lte`, joined
  by and), or the data's first and last `over` when unbounded. The lower bound limits the
  buckets, not the lookup: the value carried into the first bucket was recorded before it.
  A first bucket the lower bound cuts opens at the bound, as if it began there: over
  10 January to 20 March by month, `first` reads January's first row from the 10th on,
  else the last row before the 10th, never a row from the 1st to the 9th. `last` closes
  the last bucket at the upper bound the same way.
  `over` filtered any other way (`neq`, or under `or` / `not` in a row policy) is refused.
- **Other groupings** are read from the row each (entity, bucket) carries.
- **Bounded work.** Each (bucket, entity) cell is one index seek. Before running, the
  planner counts the cells (buckets in range by entities under the request's other
  filters, at least one, reading no measure) and refuses a request over `[validation]
  max_semi_additive_cells` (default 250,000: about 3 µs a cell on PostgreSQL 18, so near
  one second; #1459). It never truncates. 0 is refused.
- **The index** that makes each seek cheap is `(entity…, over)`:

  ```sql
  CREATE INDEX ON tf_account_day (account_id, day);
  ```

  `fraiseql doctor --against-db` warns for each fact table whose carried-forward measures no
  index leading with those keys serves, with this statement as the remedy.

### Within the bucket: `avg`, `min`, `max`, `delta`

These reduce the bucket's own rows. An entity with no row in a bucket contributes nothing
to it; a value is not carried in. `delta` is the bucket's last row minus its first (a
cumulative counter's change).

### What is refused

Each at request time, naming the measure:

- any aggregate over a `non_additive` measure;
- a reduced measure without exactly one time bucket of its `over` in `groupBy` (a calendar
  dimension is not one);
- a reduced measure alongside `count`, an additive measure, a measure reduced differently,
  or as a list (`array_agg`, `json_agg`, `jsonb_agg`, `string_agg`);
- a window aggregate (`running_sum`, `running_avg`, `running_min`, `running_max`,
  `running_stddev`, `running_variance`) over a measure that is not additive: a window runs
  over fact rows, so a running sum of a balance adds it once per day. A value function
  (`lag`, `lead`, `first_value`, `last_value`, `nth_value`) reads one row and runs.

---

## Database Requirements

Fact tables require JSONB (PostgreSQL-native binary JSON): dimension extraction uses
the `->>` operator and aggregation relies on native JSONB operator support. PostgreSQL
is the only supported backend (see [database-compatibility.md](../database-compatibility.md)).

---

## Explicit Table Declaration (Alternative to Auto-Detection)

Developers can declare fact tables explicitly instead of relying on auto-detection:

```python
fraiseql.fact_table(
    "tf_sales",
    measures=[
        {"name": "revenue", "sql_type": "numeric", "nullable": False},
        {"name": "quantity", "sql_type": "int", "nullable": False},
        {"name": "cost", "sql_type": "numeric", "nullable": False},
    ],
    dimensions={
        "name": "data",
        "paths": [
            {"name": "category", "json_path": "data->>'category'", "data_type": "text"},
            {"name": "region", "json_path": "data->>'region'", "data_type": "text"},
            {"name": "channel", "json_path": "data->>'channel'", "data_type": "text"},
        ],
    },
)
```

`fraiseql.fact_table()` is a plain function, not a decorator. The `sales_aggregate` and
`sales_window` roots follow from the `tf_sales` name; nothing is declared per query.

### A measure stored under another column

A measure is read from the column of its `name`. A pre-aggregated view that stores it under
another name declares that column:

```python
measures=[
    {"name": "volume", "sql_type": "numeric", "nullable": False, "column": "volume_total"},
]
```

The measure is still published as `volume` (`volume_sum`, `volume_avg`, …) and typed by its
`sql_type`, as every measure is: a `numeric` one is a `Decimal`, its digits exact. Every read
of it (an aggregate, `HAVING`, a window, a semi-additive reduction) reads `"volume_total"`,
quoted. `column` must be one SQL identifier, `[A-Za-z_][A-Za-z0-9_]*` within 63 bytes; the
compiler refuses anything else. `fraiseql validate facts` matches the measure against the
database on its column, and reports a declared `sql_type` the column does not have.

The `native_measures` section this replaces mapped a name to a column without a type, so
every result of such a measure was a `Float`. A schema or compiled artifact still carrying
it is refused, naming `measures[].column`.

Explicit declarations override introspected metadata and are preferred in production
deployments where sample-based discovery may be unreliable.

---

## The published roots

Each fact table `tf_<name>` publishes two `Query` fields, in introspection and in the SDL,
under their literal names (they are not camel-cased):

```graphql
sales_aggregate(where: SalesAggregateWhere, groupBy: SalesAggregateGroupBy,
                aggregates: [SalesAggregateSelection!], having: SalesAggregateHaving,
                orderBy: SalesAggregateOrderBy, limit: Int, offset: Int): [SalesAggregateRow!]!
sales_window(select: [WindowColumn!], windows: [WindowSpec!], where: SalesAggregateWhere,
             orderBy: [WindowOrderBy!], limit: Int, offset: Int): [JSON!]!
```

Both roots take the one `where` input, and both run every operator it publishes, node-id
filters included, with the same meaning.

The arguments decide what is computed, and the selection set decides what is returned:

```graphql
{
  sales_aggregate(groupBy: {category: true, occurred_at_month: true},
                  aggregates: [{revenue_sum: true}, {count: true}],
                  orderBy: {revenue_sum: DESC}) {
    category occurred_at_month revenue_sum count
  }
}
```

- `groupBy` takes `true` (grouped) or `false` per key, and `<column>_level: n` for an `LTREE`
  roll-up. `aggregates` is a list of `{<alias>: true}`. `orderBy` takes a `SortDirection` per
  key that names a grouped key or a computed aggregate: `ASC`, `DESC`, or either with the
  placement of the key's NULLs (`DESC_NULLS_LAST`, #2415), so a group whose key or aggregate
  is NULL sorts first or last.
- Every argument is read as a list query's is. An explicit `null` is the argument's absence.
  An argument published as a list (`aggregates`, `select`, `windows`, a window's `orderBy` and
  `partitionBy`) takes one object as a one-item list. Any other value of another shape is
  refused, naming the argument: `where clause must be a JSON object`, `groupBy must be an
  object`, `orderBy must be an object or array`. So is a `having` key that is not an
  aggregate and a comparison (`count_gt`).
- A `where` or `having` operand, and a window function's (`n`, `offset`), is held to the type
  its input publishes before any SQL runs, as a list query's `where` operand is: each item of
  an `_in` or `_nin`, a `Boolean` for a null check, a `String` for a pattern. One of another
  type is refused naming its path and type (`` `where.day_eq` has type `Date` ``), never the
  value. A `numeric` filter column publishes `Decimal`: its operand is its digits as a
  string, or an integer, compared exactly.
- An aggregate row holds exactly the fields its selection set selects, under their response
  keys (aliases and `__typename` included). A selected field the arguments did not compute
  is `null`. Every row field is nullable, and the root must select at least one.
- A window `orderBy` item, in a window's `orderBy` (its `OVER (ORDER BY …)`) and in the
  root's, is `{field, direction}`, and its direction takes the `SortDirection` a list query's
  does (#2415): `DESC_NULLS_LAST`, `ASC_NULLS_FIRST` and the like put the key's NULLs after
  or before every value, and `ASC` and `DESC` leave them where PostgreSQL puts them, last
  ascending and first descending. Inside `OVER` the placement
  changes the answers, not only their order: `rank` numbers the NULL rows first or last, and
  `lag` and `lead` read across them in that order.
- A window row is `JSON` keyed by the `select` and `windows` aliases. The window root takes no
  selection set. A `numeric` measure or filter column in it, selected or read through a
  window function, is the string of its digits.

A row field's type follows what the database returns:

| Field | Type |
|-------|------|
| `count`, `<measure>_count_distinct`, a tree depth | `Int` |
| `min`, `max` of an `int` measure; an `int` key | `Int` |
| `sum` of an `int` measure; `min`, `max` of a `bigint` measure; a `bigint` key | `BigInt`, exact |
| any reduction of a `numeric` measure; `sum` of a `bigint` measure; a `numeric` key | `Decimal`, exact |
| any other reduction of a measure | `Float`, approximate (a double) |
| a `date` bucket / a calendar bucket / a `timestamp` bucket / a `timestamptz` bucket | `Date` / `String` / `LocalDateTime` / `DateTime` |
| a dimension, a tree level, `<filter>_string_agg` | `String` |
| `<filter>_bool_and`, `<filter>_bool_or` | `Boolean` |
| `<filter>_array_agg` of a text filter | `[String]` |
| `<filter>_json_agg`, `<filter>_jsonb_agg`, `<measure>_json_agg`, `<measure>_jsonb_agg` | `JSON` |
| a denormalized filter used as a key | its column's scalar |

An `Int` is a signed 32-bit integer. A count outside that range is a field error at its path,
and the field is `null`. It is never a wrong number. A `BigInt` is a signed 64-bit integer
and a `Decimal` an exact number, each served as the string of its digits: PostgreSQL sums an
`int` in `bigint` and a `bigint` in `numeric`, past what a JSON number holds exactly. A
`where` or `having` operand of either takes the digits as a string or an integer.

A row type has no declaration of its own, and `@inaccessible` on its fields is not
supported.

An aggregate or window root can be one root among others in a document, beside query roots,
`__typename` or another fact-table root, each under its own alias:

```graphql
{
  orders { id }
  byRegion: sales_aggregate(groupBy: {region: true}, aggregates: [{count: true}]) { region count }
  total: sales_aggregate(aggregates: [{revenue_sum: true}]) { revenue_sum }
}
```

Each root is read by its own statement, with the gates it has alone: its fact table's
`requires_role`, the fields of the type it is read as, its arguments and its tenant scoping
are checked before any root of the document is read, and refuse the whole document as they
refuse the root alone. A root whose read then fails is a field error at its path; a fact-table
root is non-null (`[Row!]!`), so its failure makes `data` `null` (see
[Non-null completion](../features/non-null-completion.md)). A response holding a fact-table
root is never stored by an HTTP cache.

A schema whose roots cannot be published is refused when the server loads it, naming each
cause:

- a measure, dimension or filter whose name is not a GraphQL name;
- a generated key (a `_<bucket>`, `_depth` or `_level` key) spelled like a column, a dimension
  or another generated key;
- a declared type, input or enum named like one the roots publish.

---

## Read gates: the type a fact table is read as

An aggregate or a window over a fact table reads its values: a group key *is* the value, a
sum or a max is computed from them, and a filter or an order answers a question about them.
So a fact table whose columns are gated names the type it is read as, and every read gate
of that type applies:

```json
"fact_tables": {
  "tf_sales": {
    "table_name": "tf_sales",
    "type_name": "Sale",
    "measures": [{"name": "revenue", "sql_type": "Decimal", "nullable": false},
                 {"name": "margin",  "sql_type": "Decimal", "nullable": true}],
    "dimensions": {"name": "data", "paths": [{"name": "segment", "json_path": "data->>'segment'", "data_type": "text"}]},
    "denormalized_filters": [{"name": "tenant_id", "sql_type": "Text", "indexed": true}]
  }
}
```

(`type` is accepted for `type_name` in the intermediate schema.)

The two SDKs that author fact tables emit it:

```go
fraiseql.NewFactTable("data").TableName("tf_sales").TypeName("Sale"). /* measures … */ Register()
```

```typescript
SchemaRegistry.registerFactTable("tf_sales", measures, dimensions, filters, { typeName: "Sale" });
```

- Every measure, denormalized filter, dimension path and native-mapping key of the fact
  table must be a field of the type (matched by snake_case name). A link to a type that does
  not exist, or that lacks one of them, is refused when the schema loads.
- Every name a request references — `where`, `groupBy`, the aggregates and `having`,
  `orderBy`, a window's `select`, `partitionBy`, `orderBy` and function field — must be a
  field of the type (an undeclared JSONB key is refused), and one the caller may read: a
  `requires_scope` field needs the scope whether it masks or rejects, and an `authorize`
  field is never referenceable. Otherwise the request is refused (403) before any SQL runs.
- The type's `requires_role` must be held.
- A fact table with no `type_name` declares no field gate, and none applies.

Independently of the link, when a row-level security policy is configured an aggregate or a
window without a principal is refused, as a regular query is: the policy cannot be
evaluated without one.

## Tenant scoping of aggregate and window roots

An `<name>_aggregate` or `<name>_window` root has no query, so no query `inject_params` of its
own. The server scopes it by what the **fact table** declares: its own `inject_params`, those
it inherits from the type it is read as, or a relation that filters its own rows. A row policy
(`RuntimeConfig::with_rls_policy`, which an embedder installs; `fraiseql-server` installs
none) applies as well. A fact table that a schema scoping its reads by tenant leaves unscoped
would read every tenant's rows, and its roots are refused instead.

A fact table is **tenant-scoped** when either:

- the type it is read as (`type_name`) declares `inject_params`, or is returned by a query
  that does; or
- one of its `denormalized_filters` is a column some query or type injects (a `tenant_id`
  column beside list roots that inject `tenant_id`, say).

`[tenancy] mode = "row"` counts: `@tenant_id` is lowered onto the queries'
`inject_params` before this is decided.

For a tenant-scoped fact table that none of the three declarations below scopes:

- `fraiseql compile` refuses the schema, naming the fact table and the injected column or
  type that scopes it, and the declarations that would scope it.
- With no row policy installed, the server refuses its aggregate and window roots, for every
  caller, anonymous included: `Query '<root>' reads fact table '<table>', which this schema
  scopes by tenant (…), but no row policy scopes its aggregate and window roots: refused
  rather than read across tenants`. The public `Executor::execute_aggregate_query` and
  `execute_window_query` entries refuse the same way. Metadata passed to them can make a
  table tenant-scoped, never the reverse: the schema's registration of the table decides.

### The fact table's own filters: `inject_params`

A fact table declares the filters the server composes into every aggregate and window over
it, as a query declares its own:

```json
"fact_tables": [{
  "table_name": "tf_sales",
  "inject_params": {"tenant_id": {"source": "jwt", "claim": "tenant_id"}},
  "measures": [ … ],
  "dimensions": { … },
  "denormalized_filters": [{"name": "tenant_id", "sql_type": "text", "indexed": true}]
}]
```

The intermediate schema also accepts the flat form `{"tenant_id": "jwt:tenant_id"}`. The
Python (`fraiseql.fact_table(inject=…)`), TypeScript (`registerFactTable(…, { inject })`) and
Go (`NewFactTable(…).InjectParams(…)`) SDKs author it.

Each request's `WHERE` is the row policy's predicate, if any, AND each injected column equal
to its value from the caller's token, AND the caller's own `where`, so the caller's filter can
only narrow the scope. A request without a token is refused, as is a token without the claim:
the filter cannot be resolved, and the root is never read without it.

A declaration the server cannot honour does not load, naming the fact table:

- a column that is not one of its `denormalized_filters`;
- a `jwt_optional` source (an absent claim cannot filter rows: use `jwt`);
- `inject_params` beside `row_scope = "relation"` (declare one);
- a source for a column that contradicts the one its linked type declares for it.

### Inherited from the type it is read as

A fact table that declares no `inject_params` inherits its linked type's (`type_name`), when
the type declares any and each names one of the fact table's `denormalized_filters` with a
`jwt` source. Errors say which applied: `its own inject params`, or `the inject params
inherited from type 'Sale'`.

Inheritance is read off the type's own declaration only, never off a query that happens to
inject a column of the same name. A type whose `inject_params` the fact table cannot take
leaves it unscoped, refused as before, and the refusal says why.

Under a row policy, an inherited declaration adds nothing: the policy already scopes the
roots, as it did before inheritance existed. A fact table's own declaration composes with the
policy.

### A relation that filters its own rows: `row_scope = "relation"`

When the relation behind the fact table filters its rows by tenant itself, declare it:

```json
"fact_tables": [{
  "table_name": "tf_sales",
  "row_scope": "relation",
  "measures": [ … ],
  "dimensions": { … },
  "denormalized_filters": [{"name": "tenant_id", "sql_type": "text", "indexed": true}]
}]
```

The server then adds **no** predicate of its own: the relation is the only thing keeping one
tenant's rows from another's. It must filter on a value the **server** sets per request, never
on anything the client sends:

```toml
# fraiseql.toml
[[session_variables.variables]]
name = "app.tenant_id"
source = "jwt"
claim = "tenant_id"
```

```sql
CREATE VIEW tf_sales AS
SELECT * FROM tb_sales
WHERE tenant_id = NULLIF(current_setting('app.tenant_id', true), '');
```

A request with no token sets no value, so the view returns no rows. A `header` session
variable is client-controlled and must never feed such a filter.

`row_scope = "relation"` on a fact table that is not tenant-scoped changes nothing. A
relation-scoped fact table inherits no `inject_params` from its type.

---

## Aggregation Result Caching

Aggregation queries on fact tables are cached by `CachedDatabaseAdapter` using a
version-aware key strategy. Source: `cache/fact_table_cache.rs` and
`cache/fact_table_version.rs`.

### Cache Key

```text
agg:<SHA256(sql + schema_version + version_component)>
```

The `version_component` varies by strategy (see below).

### Version Strategies

Choose a strategy based on how the fact table is updated:

| Strategy | Best For | Cache Invalidation |
|----------|----------|--------------------|
| `Disabled` | Real-time accuracy required | No caching |
| `VersionTable` | ETL / batch loads with explicit version bumps | Read from `tf_versions` table; re-fetched at most every 1s |
| `TimeBased { ttl_seconds }` | Dashboards tolerant of short lag | Time-bucketed key expires after `ttl_seconds` |
| `SchemaVersion` | Immutable historical data | Cache cleared on schema deployment |

### Version Table Setup (VersionTable strategy)

```sql
CREATE TABLE tf_versions (
    table_name TEXT PRIMARY KEY,
    version    BIGINT NOT NULL DEFAULT 1,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE OR REPLACE FUNCTION bump_tf_version(p_table_name TEXT) RETURNS BIGINT AS $$
DECLARE new_version BIGINT;
BEGIN
    INSERT INTO tf_versions (table_name, version, updated_at)
    VALUES (p_table_name, 1, NOW())
    ON CONFLICT (table_name) DO UPDATE
    SET version = tf_versions.version + 1, updated_at = NOW()
    RETURNING tf_versions.version INTO new_version;
    RETURN new_version;
END;
$$ LANGUAGE plpgsql;
```

After each ETL load:

```sql
SELECT bump_tf_version('tf_sales');
```

### Configuration

```rust
use fraiseql_core::cache::fact_table_version::{
    FactTableCacheConfig, FactTableVersionStrategy,
};

let mut config = FactTableCacheConfig::default();

// ETL-loaded: explicit version bump required
config.set_strategy("tf_sales", FactTableVersionStrategy::VersionTable);

// Dashboards: 5-minute staleness acceptable
config.set_strategy(
    "tf_page_views",
    FactTableVersionStrategy::TimeBased { ttl_seconds: 300 },
);

// Historical rates: immutable until next deployment
config.set_strategy(
    "tf_historical_rates",
    FactTableVersionStrategy::SchemaVersion,
);
```

### Execution Flow

```text
execute_aggregation_query(sql)
  ↓ Extract table name (regex: "FROM tf_xxx")
  ↓ If not a fact table → execute without cache
  ↓ Look up strategy for table (falls back to default)
  ↓ If Disabled → execute without cache
  ↓ Compute version component from strategy
  ↓ Generate SHA256 cache key
  ↓ Cache hit  → return cached result
    Cache miss → execute query, store result
```

The `FactTableVersionProvider` caches version lookups with a 1-second TTL to avoid
hammering the `tf_versions` table on every query.
