# fraiseql-wire Operators Reference

Complete reference for all 25 WHERE operators and query modifiers supported by fraiseql-wire.

## Table of Contents

1. [Comparison Operators](#comparison-operators)
2. [Array Operators](#array-operators)
3. [String Operators](#string-operators)
4. [Null Handling](#null-handling)
5. [Array Length Operators](#array-length-operators)
6. [Vector Distance Operators](#vector-distance-operators-pgvector)
7. [Full-Text Search Operators](#full-text-search-operators)
8. [Network/INET Operators](#networkinet-operators)
9. [Query Modifiers](#query-modifiers)
10. [Field Sources](#field-sources)

---

## Comparison Operators

Basic comparison operators that work on any field (JSONB or direct column).

### Eq (Equal)

**PostgreSQL SQL**: `field = value`

Exact match comparison.

```rust
.where_sql("(data->>'status')::text = 'active'")
.where_sql("priority = 5")  // Direct column
```

**JSONB Type Casting**: String fields get `::text` cast automatically.

---

### Neq (Not Equal)

**PostgreSQL SQL**: `field != value` or `field <> value`

Opposite of equality.

```rust
.where_sql("(data->>'status')::text != 'archived'")
```

---

### Gt (Greater Than)

**PostgreSQL SQL**: `field > value`

Numeric or temporal comparison.

```rust
.where_sql("(data->>'priority')::numeric > 5")
.where_sql("created_at > NOW() - INTERVAL '7 days'")
```

---

### Gte (Greater Than or Equal)

**PostgreSQL SQL**: `field >= value`

---

### Lt (Less Than)

**PostgreSQL SQL**: `field < value`

---

### Lte (Less Than or Equal)

**PostgreSQL SQL**: `field <= value`

---

## Array Operators

Operators for working with PostgreSQL arrays and JSONB arrays.

### In

**PostgreSQL SQL**: `field IN (value1, value2, ...)`

Check if field matches any value in a list.

```rust
.where_sql("(data->>'status')::text IN ('active', 'pending', 'review')")
```

---

### Nin (Not In)

**PostgreSQL SQL**: `field NOT IN (...)`

Inverse of IN.

```rust
.where_sql("(data->>'status')::text NOT IN ('archived', 'deleted')")
```

---

### Contains (LIKE)

**PostgreSQL SQL**: `field LIKE '%substring%'`

Substring matching.

```rust
.where_sql("(data->>'name')::text LIKE '%Project%'")
```

**Case-Sensitive**: Yes. Use ILIKE for case-insensitive.

---

### ArrayContains (Postgres @>)

**PostgreSQL SQL**: `field @> array[value]`

Check if array contains a specific element.

```rust
.where_sql("(data->'tags') @> '\"important\"'::jsonb")
```

---

### ArrayContainedBy (Postgres <@)

**PostgreSQL SQL**: `field <@ array[value]`

Inverse of ArrayContains - check if array is contained by another.

---

### ArrayOverlaps (Postgres &&)

**PostgreSQL SQL**: `field && array[...]`

Check if arrays have any elements in common.

```rust
.where_sql("(data->'tags') && ARRAY['urgent', 'blocking']::text[]")
```

---

## String Operators

Specialized string matching operators.

### Icontains (Case-Insensitive Contains)

**PostgreSQL SQL**: `field ILIKE '%substring%'`

Case-insensitive substring matching.

```rust
.where_sql("(data->>'name')::text ILIKE '%project%'")
```

---

### Startswith

**PostgreSQL SQL**: `field LIKE 'prefix%'`

Match strings starting with a prefix.

```rust
.where_sql("(data->>'name')::text LIKE 'A%'")
```

---

### Endswith

**PostgreSQL SQL**: `field LIKE '%suffix'`

Match strings ending with a suffix.

```rust
.where_sql("(data->>'email')::text LIKE '%@example.com'")
```

---

### Like

**PostgreSQL SQL**: `field LIKE pattern`

Standard LIKE pattern matching.

```rust
.where_sql("(data->>'phone')::text LIKE '555-%'")
```

**Patterns**:

- `%` = any characters
- `_` = single character
- `\` = escape character

---

### Ilike

**PostgreSQL SQL**: `field ILIKE pattern`

Case-insensitive LIKE pattern matching.

---

## Null Handling

### IsNull

**PostgreSQL SQL**: `field IS NULL` or `field IS NOT NULL`

Check for NULL values.

```rust
.where_sql("(data->>'website') IS NULL")
.where_sql("(data->>'website') IS NOT NULL")
```

---

## Array Length Operators

Filter based on the length of arrays.

### LenEq

**PostgreSQL SQL**: `array_length(field, 1) = length`

Array has exactly N elements.

```rust
.where_sql("jsonb_array_length(data->'tags') = 3")
```

---

### LenGt (Length Greater Than)

**PostgreSQL SQL**: `array_length(field, 1) > length`

---

### LenGte (Length Greater Than or Equal)

**PostgreSQL SQL**: `array_length(field, 1) >= length`

---

### LenLt (Length Less Than)

**PostgreSQL SQL**: `array_length(field, 1) < length`

---

### LenLte (Length Less Than or Equal)

**PostgreSQL SQL**: `array_length(field, 1) <= length`

---

## Vector Distance Operators (pgvector)

**Requires**: PostgreSQL `pgvector` extension

Semantic search and similarity matching using embeddings.

### L2Distance

**PostgreSQL SQL**: `l2_distance(field::vector, vector_param::vector) < threshold`

Euclidean distance for vector similarity.

**Use Case**: Find items semantically similar to a query vector.

```rust
.where_sql("l2_distance((data->>'embedding')::vector, '[0.1, 0.2, 0.3]'::vector) < 0.5")
```

**Thresholds**: Smaller distances = more similar (0 = identical, 1+ = dissimilar)

---

### CosineDistance

**PostgreSQL SQL**: `cosine_distance(field::vector, vector_param::vector) < threshold`

Cosine distance, invariant to vector magnitude.

**Use Case**: Normalized similarity search, language embeddings.

```rust
.where_sql("cosine_distance((data->>'embedding')::vector, vector) < 0.3")
```

**Thresholds**: 0 = opposite, 0.5 = orthogonal, 1 = identical

---

### InnerProduct

**PostgreSQL SQL**: `inner_product(field::vector, vector_param::vector) > threshold`

Inner product similarity (note: uses `>` not `<`).

**Use Case**: When vectors are normalized.

```rust
.where_sql("inner_product((data->>'embedding')::vector, vector) > 0.7")
```

**Thresholds**: Higher = more similar

---

### JaccardDistance

**PostgreSQL SQL**: `jaccard_distance(field::text[], set::text[]) < threshold`

Set similarity using Jaccard index.

**Use Case**: Find similar sets of tags or categories.

```rust
.where_sql("jaccard_distance((data->'tags')::text[], ARRAY['a', 'b', 'c']) < 0.4")
```

**Thresholds**: 0 = identical, 1 = completely different

---

## Full-Text Search Operators

PostgreSQL native full-text search using `tsvector`.

### Matches

**PostgreSQL SQL**: `field @@ plainto_tsquery(language, query)`

Simple full-text search with language support.

**Use Case**: Find documents matching a natural language query.

```rust
.where_sql("(data->>'description') @@ plainto_tsquery('english', 'machine learning')")
```

**Supported Languages**: english, french, german, spanish, italian, portuguese, russian, swedish, norwegian, danish, finnish, etc.

---

### PlainQuery

**PostgreSQL SQL**: `field @@ plainto_tsquery(query)`

Full-text search without language specification.

```rust
.where_sql("(data->>'content') @@ plainto_tsquery('database')")
```

---

### PhraseQuery

**PostgreSQL SQL**: `field @@ phraseto_tsquery(language, query)`

Phrase-based full-text search.

**Use Case**: Find documents with exact phrase matches.

```rust
.where_sql("(data->>'text') @@ phraseto_tsquery('english', 'full text search')")
```

---

### WebsearchQuery

**PostgreSQL SQL**: `field @@ websearch_to_tsquery(language, query)`

Web search-style query parsing (AND, OR, NOT, quoted phrases).

**Use Case**: User-friendly full-text search similar to Google.

```rust
.where_sql("(data->>'content') @@ websearch_to_tsquery('english', '\"machine learning\" AND python')")
```

**Query Syntax**:

- `term1 term2` = AND (both terms)
- `term1 OR term2` = OR
- `term1 AND NOT term2` = exclude term2
- `"phrase here"` = exact phrase

---

## Network/INET Operators

IP address and CIDR block filtering.

### IsIPv4

**PostgreSQL SQL**: `family(field::inet) = 4`

Check if address is IPv4.

```rust
.where_sql("family((data->>'ip')::inet) = 4")
```

---

### IsIPv6

**PostgreSQL SQL**: `family(field::inet) = 6`

Check if address is IPv6.

---

### IsPrivate

**PostgreSQL SQL**: Check against RFC1918 ranges

Check if IP is in a private range.

**Private Ranges**:

- 10.0.0.0/8
- 172.16.0.0/12
- 192.168.0.0/16
- 169.254.0.0/16 (link-local)

```rust
.where_sql("((data->>'ip')::inet << '10.0.0.0/8'::inet OR (data->>'ip')::inet << '172.16.0.0/12'::inet OR (data->>'ip')::inet << '192.168.0.0/16'::inet)")
```

---

### IsLoopback

**PostgreSQL SQL**: `family(field::inet) = 4 AND field::inet << '127.0.0.0/8'::inet`

Check if IP is loopback (localhost).

**IPv4 Loopback**: 127.0.0.0/8
**IPv6 Loopback**: ::1/128

---

### InSubnet

**PostgreSQL SQL**: `field::inet << subnet::inet`

Check if IP is within a subnet.

```rust
.where_sql("(data->>'ip')::inet << '192.168.0.0/24'::inet")
```

**CIDR Notation**: Required (e.g., `192.168.0.0/24`)

---

### ContainsSubnet

**PostgreSQL SQL**: `field::inet >> subnet::inet`

Check if network contains another subnet.

---

### ContainsIP

**PostgreSQL SQL**: `field::inet >> ip::inet`

Check if network contains an IP address.

```rust
.where_sql("(data->>'network')::inet >> '192.168.1.50'::inet")
```

---

### IPRangeOverlap

**PostgreSQL SQL**: `field::inet && range::inet`

Check if IP ranges overlap.

```rust
.where_sql("(data->>'allowed_range')::inet && '192.168.0.0/16'::inet")
```

---

## Query Modifiers

Control result set size, ordering, and pagination.

### LIMIT

**PostgreSQL SQL**: `... LIMIT count`

Restrict result set to N rows.

```rust
.limit(10)
// Generates: ... LIMIT 10
```

**Use Case**: Pagination, preventing large result sets.

---

### OFFSET

**PostgreSQL SQL**: `... OFFSET count`

Skip first N rows.

```rust
.offset(20)
// Generates: ... OFFSET 20
```

**Pagination Pattern**:

```rust
.limit(per_page)
.offset((page - 1) * per_page)
```

---

### ORDER BY

**PostgreSQL SQL**: `... ORDER BY field [COLLATE collation] [ASC|DESC] [NULLS FIRST|LAST]`

Sort results by one or more fields.

```rust
// Simple JSONB field ordering
.order_by("(data->>'name') ASC")

// With collation (case-insensitive, locale-aware)
.order_by("(data->>'name') COLLATE \"en-US\" ASC")

// Multiple fields
.order_by("(data->>'status') ASC, created_at DESC")

// NULLS handling
.order_by("(data->>'website') ASC NULLS LAST")
```

**Collation Names**:

- `C` - Binary/C locale (fastest)
- `C.UTF-8` - UTF-8 binary
- `en-US`, `en_US.UTF-8` - English (US)
- `de-DE`, `de_DE.UTF-8` - German
- `fr-FR`, `fr_FR.UTF-8` - French
- `ja-JP`, `ja_JP.UTF-8` - Japanese

---

## Field Sources

### JSONB Fields

Data extracted from the `data` JSONB column.

**SQL Generation**: `(data->>'field_name')`

**Example**:

```rust
.where_sql("(data->>'name')::text = 'John'")
```

**Nested Paths**:

```rust
.where_sql("(data->'profile'->>'location')::text = 'NYC'")
```

**Type Casting**:

- String fields: `::text`
- Numeric: `::numeric` or `::integer`
- Boolean: `::boolean`
- Array length: `jsonb_array_length(data->'field')`
- Timestamps: `(data->>'created')::timestamp`

---

### Direct Columns

Database columns exposed directly (not from JSONB).

**SQL Generation**: Direct column reference

**Example**:

```rust
.where_sql("created_at > NOW() - INTERVAL '7 days'")
```

**Columns Available**:

- `id` - UUID primary key
- `created_at` - Timestamp
- `updated_at` - Timestamp

---

## Mixed Filtering

Combine JSONB and direct column filters in single query.

```rust
let results = client
    .query("projects")
    .where_sql("(data->>'status')::text = 'active'")     // JSONB filter
    .where_sql("created_at > NOW() - INTERVAL '7 days'") // Direct column
    .order_by("(data->>'priority')::numeric DESC")       // JSONB ordering
    .limit(10)
    .execute()
    .await?;
```

**Multiple Filters**: All `where_sql()` calls are AND'ed together.

---

## Performance Tips

1. **JSONB Indexes**: Create indexes on frequently filtered JSONB fields

   ```sql
   CREATE INDEX idx_status ON projects USING GIN ((data->'status'));
   ```

2. **Type Casting**: Apply minimal casting - PostgreSQL optimizes native types better

   ```rust
   // Good: Direct columns
   .where_sql("created_at > '2024-01-01'::timestamp")

   // Less optimal: JSONB with cast
   .where_sql("(data->>'timestamp')::timestamp > '2024-01-01'::timestamp")
   ```

3. **LIMIT Early**: Use LIMIT in query, not client-side

   ```rust
   // Good: Database filters
   .where_sql("(data->>'status') = 'active'")
   .limit(10)

   // Inefficient: Get all, filter client-side
   .execute()
   // ... filter in Rust
   ```

4. **ORDER BY**: Push to database, don't sort client-side

   ```rust
   // Good
   .order_by("(data->>'name') ASC")
   .limit(10)

   // Inefficient
   .execute()
   // ... sort and take 10 in Rust
   ```

5. **COLLATE**: Only use when needed for locale-aware sorting

   ```rust
   // Good: Binary sort (fastest)
   .order_by("(data->>'status') ASC")

   // When needed: Locale-aware
   .order_by("(data->>'name') COLLATE \"en-US\" ASC")
   ```

---

## Errors

### Common Error Messages

**"column does not exist"**

- Cause: Referenced a column not in the view
- Solution: Only JSONB fields available in v_* views

**"cannot cast type jsonb to..."**

- Cause: Incorrect type casting
- Solution: Use `::text` for text fields, `jsonb_array_length()` for arrays

**"does not exist (42703)"**

- Cause: Field doesn't exist in JSONB data
- Solution: Check JSON structure, use nested paths if needed

**"operator does not exist"**

- Cause: Type mismatch in comparison
- Solution: Apply correct type cast (`::text`, `::numeric`, etc.)

---

## Examples

### Search and Pagination

```rust
let results = client
    .query::<Project>("test_staging.v_projects")
    .where_sql("(data->>'status')::text = 'active'")
    .where_sql("(data->>'priority')::numeric >= 5")
    .order_by("(data->>'name') COLLATE \"en-US\" ASC")
    .limit(20)
    .offset((page - 1) * 20)
    .execute()
    .await?;
```

### Full-Text Search

```rust
let results = client
    .query::<Value>("projects")
    .where_sql("(data->>'description') @@ websearch_to_tsquery('english', 'machine learning AND python')")
    .limit(10)
    .execute()
    .await?;
```

### Vector Similarity

```rust
let results = client
    .query::<Value>("embeddings")
    .where_sql("l2_distance((data->>'vector')::vector, '[0.1,0.2,0.3]'::vector) < 0.5")
    .order_by("l2_distance((data->>'vector')::vector, '[0.1,0.2,0.3]'::vector) ASC")
    .limit(10)
    .execute()
    .await?;
```

### IP Filtering

```rust
let results = client
    .query::<Value>("access_logs")
    .where_sql("(data->>'client_ip')::inet << '192.168.0.0/24'::inet")
    .where_sql("(data->>'country')::text = 'US'")
    .limit(100)
    .execute()
    .await?;
```

---

## Related Documentation

- [Query Builder API](../README.md)
- [Streaming Guide](STREAMING.md)
- [Migration Guide](MIGRATION.md)
- [PostgreSQL INET Type](https://www.postgresql.org/docs/current/datatype-net-types.html)
- [PostgreSQL Full-Text Search](https://www.postgresql.org/docs/current/textsearch.html)
- [pgvector Documentation](https://github.com/pgvector/pgvector)
