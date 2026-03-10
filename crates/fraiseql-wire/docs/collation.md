# PostgreSQL Collation Guide

Comprehensive guide to using collations in fraiseql-wire for internationalization and locale-aware string sorting.

## What is Collation?

Collation defines how strings are compared and sorted in different languages and locales.

**Without Collation** (ASCII binary):

```
apple
APPLE
Banana  ← Uppercase comes first
banana
```

**With "en-US" Collation** (case-insensitive, English):

```
apple
APPLE   ← Treated as same position
banana
Banana  ← Treated as same position
```

**With "de-DE" Collation** (German):

```
ö comes after o
ä comes after a
ü comes after u
```

## Why Use Collation?

1. **Locale-Aware Sorting**: Respects language-specific rules
2. **Consistent Internationalization**: Different locales sort differently
3. **Case Sensitivity Control**: Case-insensitive sorting when needed
4. **Accent Handling**: How accents are ordered

## Available Collations in PostgreSQL

### Binary/Fast Collations

#### C (C Locale)

- **Speed**: Fastest
- **Sorting**: Binary/byte-by-byte
- **Case**: Sensitive (uppercase before lowercase)
- **Use**: General purpose, performance-critical

```rust
.order_by("(data->>'name') COLLATE \"C\" ASC")
```

#### C.UTF-8

- **Speed**: Fastest
- **Sorting**: UTF-8 binary
- **Case**: Sensitive
- **Use**: UTF-8 optimized version of C

```rust
.order_by("(data->>'email') COLLATE \"C.UTF-8\" ASC")
```

### Language-Specific Collations

#### English (en_US, en_GB, etc.)

```rust
// American English (default: case-insensitive)
.order_by("(data->>'name') COLLATE \"en_US.UTF-8\" ASC")

// British English
.order_by("(data->>'name') COLLATE \"en_GB.UTF-8\" ASC")
```

**Behavior**:

- Case-insensitive (apple = APPLE = Apple)
- Accents matter (café ≠ cafe)
- Follows English dictionary order

#### German (de_DE, etc.)

```rust
.order_by("(data->>'name') COLLATE \"de_DE.UTF-8\" ASC")
```

**Special Characters**:

- ä/Ä, ö/Ö, ü/Ü are recognized
- ß handled correctly

#### French (fr_FR, etc.)

```rust
.order_by("(data->>'name') COLLATE \"fr_FR.UTF-8\" ASC")
```

**Accent Rules**: French-specific accent handling

#### Spanish (es_ES, etc.)

```rust
.order_by("(data->>'name') COLLATE \"es_ES.UTF-8\" ASC")
```

**Special Characters**: Ñ/ñ, ¿, ¡ handled correctly

#### Japanese (ja_JP, etc.)

```rust
.order_by("(data->>'name') COLLATE \"ja_JP.UTF-8\" ASC")
```

**Scripts**: Hiragana, Katakana, Kanji support

#### Other Languages

- Russian: `ru_RU.UTF-8`
- Chinese: `zh_CN.UTF-8` (Simplified), `zh_TW.UTF-8` (Traditional)
- Korean: `ko_KR.UTF-8`
- Arabic: `ar_SA.UTF-8`
- Hebrew: `he_IL.UTF-8`
- Thai: `th_TH.UTF-8`
- Vietnamese: `vi_VN.UTF-8`

## How to Use in fraiseql-wire

### Single Field with Collation

```rust
client
    .query("users")
    .order_by("(data->>'name') COLLATE \"en-US\" ASC")
    .execute()
    .await?
```

### Multiple Fields with Different Collations

```rust
client
    .query("users")
    .order_by("(data->>'country') COLLATE \"C\" ASC, (data->>'name') COLLATE \"en-US\" ASC")
    .execute()
    .await?
```

### JSONB Field with Collation

```rust
// JSONB text field
.order_by("(data->>'title') COLLATE \"en-US\" ASC")

// Nested JSONB field
.order_by("(data->'profile'->>'location') COLLATE \"fr-FR\" ASC")
```

### With NULLS Handling

```rust
.order_by("(data->>'name') COLLATE \"en-US\" ASC NULLS LAST")
```

### Complete Example: Internationalized Search

```rust
async fn search_products(
    client: FraiseClient,
    query: &str,
    locale: &str,
    page: usize,
) -> Result<Vec<Product>> {
    let per_page = 20;
    let offset = (page - 1) * per_page;

    let collation = match locale {
        "de" => "de_DE.UTF-8",
        "fr" => "fr_FR.UTF-8",
        "es" => "es_ES.UTF-8",
        "ja" => "ja_JP.UTF-8",
        _ => "en-US.UTF-8",  // Default to English
    };

    let order_clause = format!(
        "(data->>'name') COLLATE \"{}\" ASC",
        collation
    );

    client
        .query::<Product>("products")
        .where_sql("(data->>'name')::text ILIKE $1")  // Search
        .where_sql("(data->>'available')::boolean = true")
        .order_by(&order_clause)
        .limit(per_page)
        .offset(offset)
        .execute()
        .await
}
```

## Collation Selection Guide

### For English Content

```rust
// Recommended: locale-aware
.order_by("(data->>'name') COLLATE \"en-US.UTF-8\" ASC")

// If performance critical:
.order_by("(data->>'name') COLLATE \"C\" ASC")
```

### For European Languages

```rust
// German
.order_by("(data->>'name') COLLATE \"de_DE.UTF-8\" ASC")

// French
.order_by("(data->>'name') COLLATE \"fr_FR.UTF-8\" ASC")

// Spanish
.order_by("(data->>'name') COLLATE \"es_ES.UTF-8\" ASC")
```

### For East Asian Content

```rust
// Japanese
.order_by("(data->>'name') COLLATE \"ja_JP.UTF-8\" ASC")

// Chinese (Simplified)
.order_by("(data->>'name') COLLATE \"zh_CN.UTF-8\" ASC")

// Korean
.order_by("(data->>'name') COLLATE \"ko_KR.UTF-8\" ASC")
```

### For Mixed/Unknown Locale

```rust
// Falls back to English
.order_by("(data->>'name') COLLATE \"en-US.UTF-8\" ASC")
```

### For Maximum Performance

```rust
// Binary sort (no locale processing)
.order_by("(data->>'name') COLLATE \"C\" ASC")
```

## Performance Considerations

### Collation Overhead

**Speed Comparison** (relative to C collation):

1. **C (fastest)**: Baseline = 1x
2. **C.UTF-8**: ~1x (same as C, optimized for UTF-8)
3. **en_US.UTF-8**: ~1.2-1.5x (Unicode aware)
4. **Complex locales** (ja_JP, zh_CN): ~2-3x (multi-byte character processing)

### Optimization Tips

1. **Use C for IDs and codes**

   ```rust
   // Good: IDs don't need collation
   .order_by("(data->>'product_id') COLLATE \"C\" ASC")

   // Less optimal:
   .order_by("(data->>'product_id') COLLATE \"en-US\" ASC")
   ```

2. **Create Indexes with Collation**

   ```sql
   -- For frequently sorted JSONB fields
   CREATE INDEX idx_users_name
   ON users
   USING BTREE ((data->>'name') COLLATE "en-US.UTF-8");
   ```

3. **Collate at Query Level, Not Column Level**
   - ✅ Good: Specify collation in `order_by()` at query time
   - ⚠️ Risky: Setting collation on column (affects all queries)

4. **Avoid Collation in WHERE Clauses**

   ```rust
   // ❌ Slower: WHERE with collation requires full scan
   .where_sql("(data->>'name') COLLATE \"en-US\" = 'John'")

   // ✅ Better: WHERE without collation, collate ORDER BY
   .where_sql("(data->>'name')::text = 'John'")
   .order_by("(data->>'name') COLLATE \"en-US\" ASC")
   ```

## Checking Available Collations

### List All Collations

```bash
# Connect to PostgreSQL
psql -U postgres -d fraiseql_test

# List all available collations
SELECT collname, collencoding FROM pg_collation
WHERE collencoding IN (-1, 6)  -- -1=all, 6=UTF-8
ORDER BY collname;
```

### Check if Collation Exists

```bash
# In psql
SELECT 1 FROM pg_collation WHERE collname = 'en_US.utf8';
```

### PostgreSQL Collation Format

Most common formats:

- `en-US.UTF-8` (hyphen)
- `en_US.UTF-8` (underscore)
- `en_US.utf8` (lowercase utf8)
- `en_US` (without encoding)

All are equivalent. Use whichever works with your PostgreSQL version.

## Common Issues and Solutions

### Issue: "collation \"en-US\" does not exist"

**Solution**: Use underscore instead of hyphen

```rust
// ❌ Won't work
.order_by("(data->>'name') COLLATE \"en-US\" ASC")

// ✅ Works
.order_by("(data->>'name') COLLATE \"en_US.UTF-8\" ASC")
```

### Issue: Collation Not Applied in WHERE

**Why**: Collation in WHERE is inefficient and often ignored

```rust
// ❌ Won't work as expected
.where_sql("(data->>'name') COLLATE \"en-US\" = 'John'")

// ✅ Use in ORDER BY instead
.where_sql("(data->>'name')::text = 'John'")
.order_by("(data->>'name') COLLATE \"en-US\" ASC")
```

### Issue: Different Results on Dev vs Production

**Cause**: Different PostgreSQL versions or collations installed

**Solution**: Explicitly specify collation, not relying on defaults

```rust
// Always explicit
.order_by("(data->>'name') COLLATE \"C.UTF-8\" ASC")
```

## Real-World Example: Multi-Language Product Catalog

```rust
async fn search_products(
    client: FraiseClient,
    query: &str,
    language: &str,
) -> Result<Vec<Product>> {
    // Map language to collation
    let collation = match language {
        "de" => "de_DE.UTF-8",
        "fr" => "fr_FR.UTF-8",
        "es" => "es_ES.UTF-8",
        "ja" => "ja_JP.UTF-8",
        "zh-cn" => "zh_CN.UTF-8",
        "ru" => "ru_RU.UTF-8",
        _ => "en_US.UTF-8",
    };

    // Search and sort by language
    let order_sql = format!(
        "(data->>'name') COLLATE \"{}\" ASC, (data->>'category') COLLATE \"{}\" ASC",
        collation, collation
    );

    client
        .query::<Product>("products")
        // Filter
        .where_sql(&format!("(data->>'language')::text = '{}'", language))
        .where_sql("(data->>'available')::boolean = true")
        // Search (case-insensitive)
        .where_sql(&format!("(data->>'description')::text ILIKE '%{}%'", query))
        // Sort with locale-aware collation
        .order_by(&order_sql)
        // Paginate
        .limit(20)
        .offset(0)
        .execute()
        .await
}
```

## Reference: PostgreSQL Collation Names

**Common Format**: `language_TERRITORY.ENCODING`

Examples:

- `en_US.UTF-8` - English, United States
- `en_GB.UTF-8` - English, Great Britain
- `de_DE.UTF-8` - German, Germany
- `de_AT.UTF-8` - German, Austria
- `de_CH.UTF-8` - German, Switzerland
- `fr_FR.UTF-8` - French, France
- `fr_CA.UTF-8` - French, Canada
- `es_ES.UTF-8` - Spanish, Spain
- `es_MX.UTF-8` - Spanish, Mexico
- `it_IT.UTF-8` - Italian, Italy
- `pt_BR.UTF-8` - Portuguese, Brazil
- `pt_PT.UTF-8` - Portuguese, Portugal
- `ru_RU.UTF-8` - Russian, Russia
- `ja_JP.UTF-8` - Japanese, Japan
- `zh_CN.UTF-8` - Chinese (Simplified), China
- `zh_TW.UTF-8` - Chinese (Traditional), Taiwan
- `ko_KR.UTF-8` - Korean, South Korea
- `tr_TR.UTF-8` - Turkish, Turkey
- `ar_SA.UTF-8` - Arabic, Saudi Arabia
- `he_IL.UTF-8` - Hebrew, Israel
- `th_TH.UTF-8` - Thai, Thailand
- `vi_VN.UTF-8` - Vietnamese, Vietnam

## Further Reading

- [PostgreSQL Collation Documentation](https://www.postgresql.org/docs/current/collation.html)
- [Unicode Collation Algorithm](https://unicode.org/reports/tr10/)
- [Windows Collation Names](https://docs.microsoft.com/en-us/sql/t-sql/statements/create-collation-transact-sql)
