# Type-Aware Filters & Advanced Querying

Production-ready filtering examples demonstrating FraiseQL's automatic filter generation based on field types. Each type automatically gets appropriate operators - no manual filter definition needed!

## What This Example Demonstrates

This is a **complete filtering pattern showcase** with:
- Automatic filter operators based on field types
- String filters (contains, startsWith, endsWith, case-insensitive)
- Numeric filters (gt, gte, lt, lte, between)
- Date/time filters (before, after, between)
- Boolean filters (equality)
- Array filters (contains, overlaps, containedBy)
- JSONB path filtering
- Complex AND/OR boolean logic
- Performance optimization tips

## Available Operators by Type

FraiseQL automatically generates appropriate filter operators based on your GraphQL schema types:

### String Operators

| Operator | Description | Example |
|----------|-------------|---------|
| `eq` | Exact match (case-sensitive) | `title: { eq: "The Hobbit" }` |
| `ne` | Not equal | `title: { ne: "Banned Book" }` |
| `in` | In list | `author: { in: ["Tolkien", "Orwell"] }` |
| `notIn` | Not in list | `author: { notIn: ["Banned"] }` |
| `contains` | Substring (case-sensitive) | `title: { contains: "Python" }` |
| `icontains` | Substring (case-insensitive) | `title: { icontains: "python" }` |
| `startsWith` | Prefix match | `title: { startsWith: "The" }` |
| `endsWith` | Suffix match | `title: { endsWith: "Guide" }` |
| `regex` | Regular expression | `title: { regex: "^[A-Z]" }` |

### Numeric Operators

| Operator | Description | Example |
|----------|-------------|---------|
| `eq` | Equal to | `price: { eq: 29.99 }` |
| `ne` | Not equal to | `price: { ne: 0.00 }` |
| `gt` | Greater than | `price: { gt: 50.00 }` |
| `gte` | Greater than or equal | `price: { gte: 20.00 }` |
| `lt` | Less than | `price: { lt: 100.00 }` |
| `lte` | Less than or equal | `price: { lte: 30.00 }` |
| `in` | In list | `pages: { in: [100, 200, 300] }` |
| `between` | Range (inclusive) | `price: { between: [20, 50] }` |

### Date/Time Operators

| Operator | Description | Example |
|----------|-------------|---------|
| `eq` | Exact match | `created_at: { eq: "2025-10-08" }` |
| `ne` | Not equal | `created_at: { ne: "2025-01-01" }` |
| `gt` / `after` | After date | `created_at: { after: "2025-01-01" }` |
| `gte` | On or after | `created_at: { gte: "2025-01-01" }` |
| `lt` / `before` | Before date | `created_at: { before: "2026-01-01" }` |
| `lte` | On or before | `created_at: { lte: "2025-12-31" }` |
| `between` | Date range | `created_at: { between: ["2025-01-01", "2025-12-31"] }` |

### Boolean Operators

| Operator | Description | Example |
|----------|-------------|---------|
| `eq` | Exact match | `in_stock: { eq: true }` |
| (direct) | Shorthand | `in_stock: true` |

### Array Operators

| Operator | Description | Example |
|----------|-------------|---------|
| `contains` | Array contains all elements | `genres: { contains: ["Fiction"] }` |
| `containedBy` | Array subset of | `genres: { containedBy: ["Fiction", "Mystery"] }` |
| `overlaps` | Array has any overlap | `genres: { overlaps: ["Fantasy", "SciFi"] }` |
| `length` | Array length | `genres: { length: 2 }` |

### Enum Operators

| Operator | Description | Example |
|----------|-------------|---------|
| `eq` | Exact match | `status: { eq: ACTIVE }` |
| `ne` | Not equal | `status: { ne: INACTIVE }` |
| `in` | In list | `status: { in: [ACTIVE, PENDING] }` |
| `notIn` | Not in list | `status: { notIn: [DELETED] }` |

## String Filtering Examples

### Basic String Filters

```graphql
# Exact match (case-sensitive)
query ExactTitle {
  books(where: { title: { eq: "The Hobbit" } }) {
    title
    author
  }
}

# Contains substring (case-sensitive)
query ContainsPython {
  books(where: { title: { contains: "Python" } }) {
    title
    author
  }
}

# Case-insensitive contains
query CaseInsensitive {
  books(where: { title: { icontains: "python" } }) {
    title
    author
  }
}

# Starts with prefix
query StartsWithThe {
  books(where: { title: { startsWith: "The" } }) {
    title
  }
}

# Ends with suffix
query EndsWithGuide {
  books(where: { title: { endsWith: "Guide" } }) {
    title
  }
}
```

### Multi-Field String Search

```graphql
query SearchTitleOrAuthor {
  books(where: {
    OR: [
      { title: { icontains: "python" } },
      { author: { icontains: "python" } }
    ]
  }) {
    title
    author
  }
}
```

### Regular Expression Filters

```graphql
# Titles starting with capital letter
query RegexFilter {
  books(where: { title: { regex: "^[A-Z]" } }) {
    title
  }
}

# ISBN format validation
query ISBNFormat {
  books(where: { isbn: { regex: "^978-[0-9]{10}$" } }) {
    title
    isbn
  }
}
```

## Numeric Filtering Examples

### Price Range Filters

```graphql
# Cheap books (under $20)
query CheapBooks {
  books(where: { price: { lt: 20.00 } }) {
    title
    price
  }
}

# Expensive books (over $50)
query ExpensiveBooks {
  books(where: { price: { gt: 50.00 } }) {
    title
    price
  }
}

# Price range $20-$40
query PriceRange {
  books(where: {
    price: { gte: 20.00, lte: 40.00 }
  }) {
    title
    price
  }
}

# Alternative: using between
query PriceRangeBetween {
  books(where: {
    price: { between: [20.00, 40.00] }
  }) {
    title
    price
  }
}
```

### Page Count Filters

```graphql
# Long books (over 500 pages)
query LongBooks {
  books(where: { pages: { gte: 500 } }) {
    title
    pages
  }
}

# Medium-length books (200-400 pages)
query MediumBooks {
  books(where: {
    pages: { gte: 200, lte: 400 }
  }) {
    title
    pages
  }
}

# Exactly 300 pages
query Exact300Pages {
  books(where: { pages: { eq: 300 } }) {
    title
    pages
  }
}
```

### Rating Filters

```graphql
# Highly-rated books (4.5+)
query HighlyRated {
  books(where: { rating: { gte: 4.5 } }) {
    title
    author
    rating
  }
}

# Books with specific ratings
query SpecificRatings {
  books(where: {
    rating: { in: [4.5, 4.8, 5.0] }
  }) {
    title
    rating
  }
}
```

## Date/Time Filtering Examples

### Recent Books

```graphql
# Books added in last 30 days
query RecentBooks {
  books(where: {
    created_at: { gte: "2025-09-08T00:00:00Z" }
  }) {
    title
    created_at
  }
}

# Books added after specific date
query AfterDate {
  books(where: {
    created_at: { after: "2025-01-01T00:00:00Z" }
  }) {
    title
    created_at
  }
}

# Books added in date range
query DateRange {
  books(where: {
    created_at: {
      after: "2025-01-01T00:00:00Z",
      before: "2025-12-31T23:59:59Z"
    }
  }) {
    title
    created_at
  }
}
```

### Publication Year Filters

```graphql
# Classic books (published before 1950)
query ClassicBooks {
  books(where: { published_year: { lt: 1950 } }) {
    title
    author
    published_year
  }
}

# Modern books (2020 or later)
query ModernBooks {
  books(where: { published_year: { gte: 2020 } }) {
    title
    published_year
  }
}
```

## Array Filtering Examples

### Genre Filters

```graphql
# Books with "Science Fiction" genre
query SciFiBooks {
  books(where: {
    genres: { contains: ["Science Fiction"] }
  }) {
    title
    genres
  }
}

# Books with BOTH "Mystery" AND "Thriller"
query MysteryThrillers {
  books(where: {
    genres: { contains: ["Mystery", "Thriller"] }
  }) {
    title
    genres
  }
}

# Books with ANY of these genres (overlap)
query FantasyOrAdventure {
  books(where: {
    genres: { overlaps: ["Fantasy", "Adventure"] }
  }) {
    title
    genres
  }
}

# Books ONLY in these genres (subset)
query OnlyFictionOrMystery {
  books(where: {
    genres: { containedBy: ["Fiction", "Mystery"] }
  }) {
    title
    genres
  }
}
```

### Array Length Filters

```graphql
# Books with exactly 2 genres
query TwoGenres {
  books(where: {
    genres: { length: 2 }
  }) {
    title
    genres
  }
}

# Books with multiple genres (3+)
query MultiGenre {
  books(where: {
    genres: { length: { gte: 3 } }
  }) {
    title
    genres
  }
}
```

## Boolean Filtering

```graphql
# In-stock books
query InStockBooks {
  books(where: { in_stock: true }) {
    title
    price
  }
}

# Out-of-stock books
query OutOfStock {
  books(where: { in_stock: false }) {
    title
  }
}

# Alternative explicit syntax
query InStockExplicit {
  books(where: { in_stock: { eq: true } }) {
    title
  }
}
```

## Enum Filtering

```graphql
# VIP members only
query VIPMembers {
  members(where: { membership_tier: { eq: VIP } }) {
    name
    email
    membership_tier
  }
}

# Premium and VIP members
query PremiumAndVIP {
  members(where: {
    membership_tier: { in: [PREMIUM, VIP] }
  }) {
    name
    membership_tier
  }
}

# Non-basic members
query NonBasic {
  members(where: {
    membership_tier: { ne: BASIC }
  }) {
    name
    membership_tier
  }
}
```

## Complex Boolean Logic (AND/OR)

### AND Logic (All Conditions Must Match)

```graphql
# Science Fiction books, in stock, under $30, 4+ rating
query ComplexAND {
  books(where: {
    AND: [
      { genres: { contains: ["Science Fiction"] } },
      { in_stock: true },
      { price: { lte: 30.00 } },
      { rating: { gte: 4.0 } }
    ]
  }) {
    title
    price
    rating
    genres
    in_stock
  }
}
```

### OR Logic (Any Condition Can Match)

```graphql
# Books by multiple favorite authors
query FavoriteAuthors {
  books(where: {
    OR: [
      { author: { eq: "J.R.R. Tolkien" } },
      { author: { eq: "George Orwell" } },
      { author: { eq: "Harper Lee" } }
    ]
  }) {
    title
    author
  }
}
```

### Combined AND/OR Logic

```graphql
# Science Fiction OR Fantasy, AND in stock, AND affordable
query ComplexLogic {
  books(where: {
    AND: [
      {
        OR: [
          { genres: { contains: ["Science Fiction"] } },
          { genres: { contains: ["Fantasy"] } }
        ]
      },
      { in_stock: true },
      { price: { lte: 30.00 } }
    ]
  }) {
    title
    genres
    price
  }
}
```

### Nested Boolean Logic

```graphql
# (SciFi OR Fantasy) AND (cheap OR highly-rated) AND in-stock
query NestedLogic {
  books(where: {
    AND: [
      {
        OR: [
          { genres: { overlaps: ["Science Fiction"] } },
          { genres: { overlaps: ["Fantasy"] } }
        ]
      },
      {
        OR: [
          { price: { lte: 20.00 } },
          { rating: { gte: 4.5 } }
        ]
      },
      { in_stock: true }
    ]
  }) {
    title
    genres
    price
    rating
  }
}
```

## Nested Filtering (Relationships)

### Filter by Related Records

```graphql
# Members with active subscriptions
query MembersWithActiveSubscription {
  members(where: {
    subscription: {
      status: { eq: "active" }
    }
  }) {
    name
    email
    subscription {
      status
      expires_at
    }
  }
}

# Books by authors with 5+ published books
query ProlificAuthors {
  books(where: {
    author: {
      books_count: { gte: 5 }
    }
  }) {
    title
    author {
      name
      books_count
    }
  }
}
```

## JSON Path Filtering (JSONB)

### Filter by JSONB Fields

```graphql
# Devices in production environment
query ProductionDevices {
  devices(where: {
    tags: { path: "$.environment", equals: "production" }
  }) {
    hostname
    tags
  }
}

# Devices with monitoring enabled
query MonitoredDevices {
  devices(where: {
    tags: { path: "$.monitoring.enabled", equals: true }
  }) {
    hostname
    tags
  }
}
```

### JSONB Containment

```sql
-- PostgreSQL JSONB containment (in raw SQL)
SELECT * FROM devices
WHERE tags @> '{"environment": "production"}'::jsonb;

-- Check if key exists
SELECT * FROM devices
WHERE tags ? 'monitoring';

-- Check nested path
SELECT * FROM devices
WHERE tags->'monitoring'->>'enabled' = 'true';
```

## Performance Tips

### 1. Index Your Filter Fields

Create indexes on columns you frequently filter:

```sql
-- Single-column indexes
CREATE INDEX idx_books_author ON books(author);
CREATE INDEX idx_books_price ON books(price);
CREATE INDEX idx_books_in_stock ON books(in_stock) WHERE in_stock = true;

-- Composite indexes for common combinations
CREATE INDEX idx_books_genre_price ON books USING gin (genres), price;

-- Full-text search indexes
CREATE INDEX idx_books_title_fts ON books
    USING gin (to_tsvector('english', title));

-- JSONB indexes
CREATE INDEX idx_devices_tags ON devices USING gin (tags);
```

### 2. Use Composite Indexes for AND Queries

When filtering on multiple fields together, use composite indexes:

```sql
-- Common query: filter by category AND price range
CREATE INDEX idx_products_category_price
    ON products(category_id, price);

-- Common query: filter by status AND date
CREATE INDEX idx_orders_status_date
    ON orders(status, created_at DESC);
```

### 3. Limit Result Sets

Always use pagination:

```graphql
query PaginatedBooks {
  books(
    where: { in_stock: true }
    limit: 50
    offset: 0
    orderBy: { created_at: DESC }
  ) {
    title
    author
    price
  }
}
```

### 4. Avoid Leading Wildcards

```graphql
# ✅ FAST: Uses index
query StartsWithThe {
  books(where: { title: { startsWith: "The" } }) {
    title
  }
}

# ⚠️ SLOWER: Can't use regular B-tree index
query EndsWithGuide {
  books(where: { title: { endsWith: "Guide" } }) {
    title
  }
}

# ❌ SLOW: Full table scan
query ContainsMiddle {
  books(where: { title: { contains: "middle" } }) {
    title
  }
}
```

**Solution:** Use full-text search for substring matching:

```sql
-- Create full-text search index
CREATE INDEX idx_books_title_fts
    ON books USING gin (to_tsvector('english', title));

-- Query with full-text search (FAST)
SELECT * FROM books
WHERE to_tsvector('english', title) @@ to_tsquery('english', 'python');
```

### 5. Filter Before Sorting

PostgreSQL optimizes better when filters come before ORDER BY:

```graphql
# ✅ GOOD: Filter first, then sort
query OptimizedQuery {
  books(
    where: {
      in_stock: true
      price: { lte: 30.00 }
    }
    orderBy: { created_at: DESC }
    limit: 20
  ) {
    title
  }
}
```

### 6. Use Partial Indexes

For frequently-queried subsets:

```sql
-- Only index active books (saves space and is faster)
CREATE INDEX idx_books_active
    ON books(category_id, price)
    WHERE in_stock = true;

-- Only index recent orders
CREATE INDEX idx_orders_recent
    ON orders(customer_id, status)
    WHERE created_at > NOW() - INTERVAL '90 days';
```

### 7. Monitor Query Performance

Use EXPLAIN ANALYZE to verify index usage:

```sql
EXPLAIN ANALYZE
SELECT * FROM books
WHERE in_stock = true
  AND price BETWEEN 20 AND 50
ORDER BY created_at DESC
LIMIT 20;

-- Look for:
-- ✅ "Index Scan" or "Index Only Scan"
-- ❌ "Seq Scan" (means no index used)
```

## Database Schema

```sql
-- Books table with comprehensive indexes
CREATE TABLE tb_books (
    id SERIAL PRIMARY KEY,
    title VARCHAR(500) NOT NULL,
    author VARCHAR(200) NOT NULL,
    isbn VARCHAR(20) UNIQUE,
    published_year INT,
    pages INT NOT NULL,
    price DECIMAL(10,2) NOT NULL,
    genres TEXT[] NOT NULL DEFAULT '{}',
    in_stock BOOLEAN NOT NULL DEFAULT true,
    language VARCHAR(50) NOT NULL DEFAULT 'English',
    rating DECIMAL(3,2),
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Performance indexes for filtering
CREATE INDEX idx_books_author ON tb_books(author);
CREATE INDEX idx_books_price ON tb_books(price);
CREATE INDEX idx_books_rating ON tb_books(rating) WHERE rating IS NOT NULL;
CREATE INDEX idx_books_published ON tb_books(published_year);
CREATE INDEX idx_books_created ON tb_books(created_at DESC);

-- Full-text search indexes
CREATE INDEX idx_books_title_fts
    ON tb_books USING gin (to_tsvector('english', title));
CREATE INDEX idx_books_author_fts
    ON tb_books USING gin (to_tsvector('english', author));

-- Array index for genres
CREATE INDEX idx_books_genres ON tb_books USING gin (genres);

-- Partial index for in-stock books
CREATE INDEX idx_books_in_stock
    ON tb_books(price, created_at)
    WHERE in_stock = true;

-- Composite index for common query pattern
CREATE INDEX idx_books_language_price
    ON tb_books(language, price);

-- View for GraphQL
CREATE VIEW v_books AS
SELECT
    id,
    title,
    author,
    isbn,
    published_year,
    pages,
    price,
    genres,
    in_stock,
    language,
    rating,
    created_at,
    updated_at
FROM tb_books;

-- Members table
CREATE TABLE tb_members (
    id SERIAL PRIMARY KEY,
    email VARCHAR(255) UNIQUE NOT NULL,
    name VARCHAR(200) NOT NULL,
    membership_tier VARCHAR(20) NOT NULL,
    joined_date TIMESTAMP NOT NULL DEFAULT NOW(),
    is_active BOOLEAN NOT NULL DEFAULT true,
    books_borrowed INT NOT NULL DEFAULT 0,
    CONSTRAINT valid_tier CHECK (
        membership_tier IN ('basic', 'premium', 'vip')
    )
);

CREATE INDEX idx_members_tier ON tb_members(membership_tier);
CREATE INDEX idx_members_active ON tb_members(is_active) WHERE is_active = true;

CREATE VIEW v_members AS SELECT * FROM tb_members;
```

## Setup

### 1. Install Dependencies

```bash
cd examples/filtering
pip install -r requirements.txt
```

### 2. Setup Database

```bash
# Create database
createdb library

# Apply schema
psql library < schema.sql
```

### 3. Load Sample Data

```sql
INSERT INTO tb_books (title, author, isbn, published_year, pages, price, genres, rating) VALUES
('The Hobbit', 'J.R.R. Tolkien', '9780547928227', 1937, 310, 14.99, ARRAY['Fantasy', 'Adventure'], 4.8),
('1984', 'George Orwell', '9780451524935', 1949, 328, 15.99, ARRAY['Dystopian', 'Fiction'], 4.7),
('To Kill a Mockingbird', 'Harper Lee', '9780061120084', 1960, 324, 18.99, ARRAY['Fiction', 'Classic'], 4.8),
('The Great Gatsby', 'F. Scott Fitzgerald', '9780743273565', 1925, 180, 12.99, ARRAY['Fiction', 'Classic'], 4.4),
('Dune', 'Frank Herbert', '9780441172719', 1965, 688, 19.99, ARRAY['Science Fiction', 'Adventure'], 4.5),
('Harry Potter', 'J.K. Rowling', '9780439708180', 1997, 309, 24.99, ARRAY['Fantasy', 'Adventure', 'Young Adult'], 4.9),
('The Catcher in the Rye', 'J.D. Salinger', '9780316769174', 1951, 234, 13.99, ARRAY['Fiction', 'Classic'], 4.0);
```

### 4. Run the Application

```bash
python main.py
```

Access at http://localhost:8000/graphql

## Comparison with Other Frameworks

### FraiseQL (Automatic Filters)

```graphql
# No filter definition needed - automatic based on types!
query AutoFilters {
  books(where: {
    title: { icontains: "python" }
    price: { gte: 20, lte: 50 }
    in_stock: true
    genres: { overlaps: ["Programming"] }
  }) {
    title
    price
  }
}
```

### Other Frameworks (Manual Filter Definition)

```python
# Manual filter definition required in other frameworks
class BookFilter:
    title_contains = String()
    title_icontains = String()
    price_gte = Decimal()
    price_lte = Decimal()
    in_stock = Boolean()
    genres_contains = List(String)
    # ... must manually define every operator for every field
```

**FraiseQL advantage:** Filters are automatically generated based on your types!

## Related Examples

- [`../hybrid_tables/`](../hybrid_tables/) - Combining indexed columns with JSONB
- [`../specialized_types/`](../specialized_types/) - PostgreSQL-specific types
- [`../fastapi/`](../fastapi/) - Complete FastAPI integration

## Key Takeaways

1. **Automatic filter generation** - No manual filter definition needed
2. **Type-aware operators** - Each type gets appropriate filters
3. **Powerful boolean logic** - Complex AND/OR combinations supported
4. **Array operations** - contains, overlaps, containedBy for arrays
5. **JSONB path filtering** - Query nested JSON fields
6. **Index your filters** - Create indexes on frequently-filtered columns
7. **Use EXPLAIN ANALYZE** - Verify your queries use indexes
8. **Paginate results** - Always use limit/offset for large datasets

---

**FraiseQL's automatic type-aware filtering means you get powerful querying capabilities without writing filter definitions. Define your types, and filters are generated automatically!** 🔍
