# WHERE Clause Operators Reference

**Version**: FraiseQL v1.8+
**Status:** Complete
**Total Operators**: 150+
**Categories**: 15 operator categories

---

## Overview

FraiseQL provides 150+ WHERE clause operators for filtering, searching, and comparing data across all supported column types. These operators enable:

- **Type-safe filtering**: Operators validated at GraphQL execution time
- **Database efficiency**: Direct SQL translation with optimal query plans
- **Complex queries**: Boolean logic (AND/OR/NOT) with nested conditions
- **Specialized operations**: Geographic distance, vector similarity, hierarchical paths, full-text search

All operators are type-aware and only work with compatible column types. Invalid operator/type combinations return GraphQL errors at query time.

---

## Operator Categories

### 1. Basic Comparison Operators

Basic comparison operators work with all comparable types (numeric, string, date, etc.).

#### **Equality Operators**

| Operator | SQL Equivalent | Description | Example |
|----------|---|---|---|
| `eq` | `=` | Equality | `{age: {eq: 25}}` |
| `neq` | `!=` / `<>` | Not equal | `{status: {neq: "inactive"}}` |

**Supported Types**: All types (strings, numbers, dates, UUIDs, etc.)

**Examples**:
```graphql
# Numeric
{age: {eq: 25}}
{price: {eq: 99.99}}

# String
{status: {eq: "active"}}
{email: {eq: "user@example.com"}}

# Date
{birthDate: {eq: "1990-05-15"}}

# UUID
{id: {eq: "550e8400-e29b-41d4-a716-446655440000"}}

# Boolean
{isActive: {eq: true}}
```

#### **Comparison Operators**

| Operator | SQL Equivalent | Description | Example |
|----------|---|---|---|
| `gt` | `>` | Greater than | `{age: {gt: 18}}` |
| `gte` | `>=` | Greater than or equal | `{age: {gte: 21}}` |
| `lt` | `<` | Less than | `{age: {lt: 65}}` |
| `lte` | `<=` | Less than or equal | `{age: {lte: 65}}` |

**Supported Types**: Numeric (int, float, decimal), Date, DateTime, String (lexical comparison)

**Examples**:
```graphql
# Numeric ranges
{age: {gte: 18, lte: 65}}
{price: {gt: 100, lt: 500}}

# Date ranges
{createdAt: {gte: "2024-01-01", lt: "2025-01-01"}}
{birthDate: {lte: "2007-01-01"}}

# String lexical comparison
{name: {gte: "A", lt: "B"}}
```

---

### 2. String/Text Operators

String operators provide flexible text searching with case-sensitivity control and pattern matching.

#### **Substring Operators**

| Operator | SQL Equivalent | Description | Case-Sensitive | Example |
|----------|---|---|---|---|
| `contains` | `LIKE '%value%'` | Contains substring | Yes | `{name: {contains: "John"}}` |
| `icontains` | `ILIKE '%value%'` | Contains substring | No | `{name: {icontains: "john"}}` |
| `startswith` | `LIKE 'value%'` | Starts with | Yes | `{name: {startswith: "J"}}` |
| `istartswith` | `ILIKE 'value%'` | Starts with | No | `{name: {istartswith: "j"}}` |
| `endswith` | `LIKE '%value'` | Ends with | Yes | `{name: {endswith: "son"}}` |
| `iendswith` | `ILIKE '%value'` | Ends with | No | `{name: {iendswith: "SON"}}` |

**Supported Types**: String, Text, Slug, domains, URLs

**Examples**:
```graphql
# Case-sensitive
{email: {contains: "@example.com"}}
{title: {startswith: "The"}}
{filename: {endswith: ".pdf"}}

# Case-insensitive
{name: {icontains: "smith"}}
{city: {istartswith: "san"}}

# Combined
{email: {contains: "@"}}
{url: {startswith: "https://"}}
```

#### **Pattern Operators**

| Operator | SQL Equivalent | Description | Pattern Support | Example |
|----------|---|---|---|---|
| `like` | `LIKE` | User-provided pattern | SQL wildcards (% = any, _ = single) | `{name: {like: "%J_hn%"}}` |
| `ilike` | `ILIKE` | Case-insensitive pattern | SQL wildcards | `{name: {ilike: "%SMITH%"}}` |

**Wildcard Rules**:
- `%` = Any characters (0 or more)
- `_` = Exactly one character
- Escape with backslash: `\%` for literal percent

**Examples**:
```graphql
# Find names with pattern
{name: {like: "John%"}}        # Starts with John
{name: {like: "%Smith"}}       # Ends with Smith
{name: {like: "%Robert%"}}     # Contains Robert
{name: {like: "A_C"}}          # Three-letter, starts with A, ends with C

# Case-insensitive
{email: {ilike: "%@example.com"}}
```

#### **Regular Expression Operators**

| Operator | SQL Equivalent | Description | Example |
|----------|---|---|---|
| `matches` | `~` | Regex match (case-sensitive) | `{email: {matches: ".*@example\\.com$"}}` |
| `imatches` | `~*` | Regex match (case-insensitive) | `{email: {imatches: ".*@example\\.com$"}}` |
| `not_matches` | `!~` | Negated regex match | `{email: {not_matches: ".*@spam\\.com$"}}` |

**PostgreSQL POSIX Regular Expressions**:
- `.` = Any character
- `*` = Zero or more
- `+` = One or more
- `?` = Zero or one
- `^` = Start of string
- `$` = End of string
- `[...]` = Character class
- `(...)` = Group
- `|` = OR

**Examples**:
```graphql
# Email validation pattern
{email: {matches: "^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\.[a-zA-Z]{2,}$"}}

# Phone number pattern
{phone: {matches: "^\\+?[1-9]\\d{1,14}$"}}

# Exclude pattern
{email: {not_matches: ".*@(spam|test)\\.com$"}}

# Case-insensitive domain check
{email: {imatches: ".*@EXAMPLE\\.COM$"}}

# URL protocol check
{url: {matches: "^https?://"}}
```

---

### 3. Containment/List Operators

List operators check if values are in a provided list.

#### **List Operators**

| Operator | SQL Equivalent | Description | Example |
|----------|---|---|---|
| `in` | `IN (...)` | Value in list | `{status: {in: ["active", "pending"]}}` |
| `nin` / `notin` | `NOT IN (...)` | Value not in list | `{status: {nin: ["deleted", "archived"]}}` |

**Supported Types**: All types (numeric, string, UUID, date, etc.)

**Examples**:
```graphql
# String values
{status: {in: ["active", "pending", "approved"]}}
{country: {nin: ["XX", "YY"]}}

# Numeric values
{userId: {in: [1, 2, 3, 4, 5]}}
{priority: {nin: [0, -1]}}

# UUID values
{parentId: {in: ["550e8400-e29b-41d4-a716-446655440000", "abcd1234-e29b-41d4-a716-446655440000"]}}

# Date values
{status: {in: ["2024-01-01", "2024-12-31"]}}

# Mixed with other operators
{status: {in: ["active", "pending"]}, age: {gte: 18}}
```

---

### 4. NULL Checking

#### **NULL Operators**

| Operator | SQL Equivalent | Description | Example |
|----------|---|---|---|
| `isnull` | `IS NULL` / `IS NOT NULL` | Check if NULL | `{deletedAt: {isnull: true}}` |

**Supported Types**: All types

**Examples**:
```graphql
# Records with NULL values
{deletedAt: {isnull: true}}
{middleName: {isnull: true}}

# Records without NULL values
{email: {isnull: false}}
{phone: {isnull: false}}

# Soft delete pattern
{AND: [{isnull: false}, {deletedAt: {isnull: true}}]}
```

---

### 5. Array Operators

Array operators work with JSONB array columns (columns storing JSON arrays).

#### **Array Comparison**

| Operator | SQL Equivalent | Description | Example |
|----------|---|---|---|
| `eq` / `array_eq` | `=` | Array equality | `{tags: {eq: ["a", "b"]}}` |
| `neq` / `array_neq` | `!=` | Array inequality | `{tags: {neq: ["x"]}}` |

**Examples**:
```graphql
{tags: {eq: ["important", "review"]}}
{items: {neq: []}}
```

#### **Array Containment**

| Operator | SQL Equivalent | Description | Example |
|----------|---|---|---|
| `contains` / `array_contains` | `@>` | Array contains all elements | `{tags: {contains: ["important"]}}` |
| `contained_by` / `array_contained_by` | `<@` | Array is contained by | `{tags: {contained_by: ["a", "b", "c"]}}` |
| `overlaps` / `array_overlaps` | `&&` | Arrays have common elements | `{tags: {overlaps: ["urgent", "review"]}}` |

**Containment vs Overlaps**:
- `contains`: Subject array must have ALL elements of provided array
- `overlaps`: Subject array must have AT LEAST ONE element of provided array
- `contained_by`: Subject array must be subset of provided array

**Examples**:
```graphql
# Must have ALL these tags
{tags: {contains: ["important", "urgent"]}}

# Must overlap with any of these
{tags: {overlaps: ["todo", "review", "pending"]}}

# Must be subset of allowed tags
{tags: {contained_by: ["dev", "qa", "prod", "staging"]}}
```

#### **Array Length**

| Operator | SQL Equivalent | Description | Example |
|----------|---|---|---|
| `len_eq` / `array_length_eq` | `array_length(,1) =` | Length equals | `{items: {len_eq: 5}}` |
| `len_neq` / `array_length_neq` | `array_length(,1) !=` | Length not equal | `{items: {len_neq: 0}}` |
| `len_gt` / `array_length_gt` | `array_length(,1) >` | Length greater than | `{items: {len_gt: 3}}` |
| `len_gte` / `array_length_gte` | `array_length(,1) >=` | Length >= | `{items: {len_gte: 1}}` |
| `len_lt` / `array_length_lt` | `array_length(,1) <` | Length less than | `{items: {len_lt: 10}}` |
| `len_lte` / `array_length_lte` | `array_length(,1) <=` | Length <= | `{items: {len_lte: 20}}` |

**Examples**:
```graphql
# Array length checks
{items: {len_eq: 5}}           # Exactly 5 items
{tags: {len_gte: 1}}           # At least 1 tag
{attachments: {len_lt: 100}}   # Fewer than 100
{reviews: {len_neq: 0}}        # Non-empty reviews
```

#### **Array Element Matching**

| Operator | SQL Equivalent | Description | Example |
|----------|---|---|---|
| `any_eq` / `array_any_eq` | `= ANY(array)` | Any element equals | `{items: {any_eq: "important"}}` |
| `all_eq` / `array_all_eq` | `= ALL(array)` | All elements equal | `{items: {all_eq: "same"}}` |

**Examples**:
```graphql
# Any element matches
{items: {any_eq: "completed"}}

# All elements match (rarely useful, for uniform arrays)
{statuses: {all_eq: "active"}}
```

---

### 6. Network/IP Address Operators

Network operators work with INET and CIDR PostgreSQL types (IPv4 and IPv6 addresses/networks).

#### **IP Address Comparison**

| Operator | SQL Equivalent | Description | Example |
|----------|---|---|---|
| `eq` | `=` | IP equality | `{ip: {eq: "192.168.1.1"}}` |
| `neq` | `!=` | IP inequality | `{ip: {neq: "10.0.0.1"}}` |
| `in` | `IN (...)` | IP in list | `{ip: {in: ["192.168.1.1", "10.0.0.1"]}}` |
| `nin` / `notin` | `NOT IN (...)` | IP not in list | `{ip: {nin: ["10.0.0.0/8"]}}` |

**Examples**:
```graphql
# Exact IP match
{ip: {eq: "192.168.1.100"}}

# IPv6
{ip: {eq: "2001:db8::1"}}

# IP in list
{sourceIp: {in: ["192.168.1.1", "192.168.1.2", "192.168.1.3"]}}

# Exclude ranges
{ip: {nin: ["10.0.0.0/8", "172.16.0.0/12"]}}
```

#### **IP Address Classification**

| Operator | SQL Equivalent | Description | Example |
|----------|---|---|---|
| `isprivate` / `isPrivate` | RFC 1918 check | Is private IP | `{ip: {isprivate: true}}` |
| `ispublic` / `isPublic` | NOT RFC 1918 check | Is public IP | `{ip: {ispublic: true}}` |
| `isipv4` / `isIPv4` | `family() = 4` | Is IPv4 address | `{ip: {isipv4: true}}` |
| `isipv6` / `isIPv6` | `family() = 6` | Is IPv6 address | `{ip: {isipv6: true}}` |

**Private IP Ranges (RFC 1918)**:
- `10.0.0.0/8` (10.0.0.0 to 10.255.255.255)
- `172.16.0.0/12` (172.16.0.0 to 172.31.255.255)
- `192.168.0.0/16` (192.168.0.0 to 192.168.255.255)
- `127.0.0.0/8` (Loopback)
- `169.254.0.0/16` (Link-local)
- Etc.

**Examples**:
```graphql
# Find public IPs (security audits)
{ip: {ispublic: true}}

# Find private/internal IPs
{ip: {isprivate: true}}

# Filter by IP version
{ip: {isipv4: true}}          # IPv4 only
{ip: {isipv6: true}}          # IPv6 only

# Combined filtering
{AND: [{ip: {isipv4: true}}, {ip: {isprivate: true}}]}
```

#### **Network Range Operators**

| Operator | SQL Equivalent | Description | Example |
|----------|---|---|---|
| `insubnet` / `inSubnet` | `<<=` | IP in subnet | `{ip: {insubnet: "192.168.0.0/16"}}` |
| `inrange` / `inRange` | `<<=` | IP in CIDR range (alias) | `{ip: {inrange: "10.0.0.0/8"}}` |
| `overlaps` | `&&` | Networks overlap | `{network: {overlaps: "192.168.0.0/24"}}` |
| `strictleft` | `<<` | Network strictly left of | `{network: {strictleft: "192.169.0.0/16"}}` |
| `strictright` | `>>` | Network strictly right of | `{network: {strictright: "192.167.0.0/16"}}` |

**Examples**:
```graphql
# Check if IP is in corporate network
{ip: {insubnet: "203.0.113.0/24"}}

# Check if in restricted range
{sourceIp: {inrange: "10.0.0.0/8"}}

# Network overlap check
{network: {overlaps: "192.168.0.0/24"}}

# Ordering checks
{network: {strictleft: "192.170.0.0/16"}}
```

---

### 7. MAC Address Operators

MAC address operators work with `macaddr` PostgreSQL type.

| Operator | SQL Equivalent | Description | Example |
|----------|---|---|---|
| `eq` | `=` | MAC equality | `{mac: {eq: "08:00:2b:01:02:03"}}` |
| `neq` | `!=` | MAC inequality | `{mac: {neq: "ff:ff:ff:ff:ff:ff"}}` |
| `in` | `IN (...)` | MAC in list | `{mac: {in: ["00:11:22:33:44:55"]}}` |
| `nin` / `notin` | `NOT IN (...)` | MAC not in list | `{mac: {nin: ["ff:ff:ff:ff:ff:ff"]}}` |
| `isnull` | `IS NULL` | Check if NULL | `{mac: {isnull: false}}` |

**Format**: Colon-separated hexadecimal octets

**Examples**:
```graphql
# Device identification
{mac: {eq: "a0:1d:48:12:34:56"}}

# Allowlist
{mac: {in: ["08:00:2b:01:02:03", "0c:42:a1:23:45:67"]}}

# Exclude broadcast
{mac: {neq: "ff:ff:ff:ff:ff:ff"}}

# Exclude DHCP
{mac: {isnull: false}}
```

---

### 8. Date Range Operators

Date range operators work with PostgreSQL `daterange` type.

#### **Date Range Comparison**

| Operator | SQL Equivalent | Description | Example |
|----------|---|---|---|
| `eq` | `=` | Range equality | `{period: {eq: "[2024-01-01, 2024-12-31]"}}` |
| `neq` | `!=` | Range inequality | `{period: {neq: "[2023-01-01, 2023-12-31]"}}` |
| `in` | `IN (...)` | Range in list | `{period: {in: ["[2024-01-01, 2024-12-31]", "[2025-01-01, 2025-12-31]"]}}` |
| `nin` / `notin` | `NOT IN (...)` | Range not in list | `{period: {nin: ["[2023-01-01, 2023-12-31]"]}}` |

**Range Notation**:
- `[start, end]` = Inclusive on both sides
- `(start, end)` = Exclusive on both sides
- `[start, end)` = Inclusive start, exclusive end
- `(start, end]` = Exclusive start, inclusive end

**Examples**:
```graphql
# Year 2024 (inclusive)
{period: {eq: "[2024-01-01, 2024-12-31]"}}

# Excluding year 2023
{period: {nin: ["[2023-01-01, 2023-12-31]"]}}
```

#### **Date Range Containment**

| Operator | SQL Equivalent | Description | Example |
|----------|---|---|---|
| `contains_date` | `@>` | Range contains date | `{period: {contains_date: "2024-06-15"}}` |
| `overlaps` | `&&` | Ranges overlap | `{period: {overlaps: "[2024-01-01, 2024-12-31]"}}` |
| `adjacent` | `-\|-` | Ranges adjacent | `{period: {adjacent: "[2025-01-01, 2025-12-31]"}}` |
| `strictly_left` | `<<` | Range strictly left of | `{period: {strictly_left: "[2025-01-01, 2025-12-31]"}}` |
| `strictly_right` | `>>` | Range strictly right of | `{period: {strictly_right: "[2023-01-01, 2023-12-31]"}}` |
| `not_left` | `&>` | Range does not extend left | `{period: {not_left: "[2024-06-01, 2025-12-31]"}}` |
| `not_right` | `&<` | Range does not extend right | `{period: {not_right: "[2023-01-01, 2024-06-30]"}}` |

**Examples**:
```graphql
# Events during 2024
{period: {contains_date: "2024-06-15"}}

# Overlapping projects
{period: {overlaps: "[2024-03-01, 2024-09-30]"}}

# Adjacent phases
{period: {adjacent: "[2024-Q2, 2024-Q3]"}}

# Before date
{period: {strictly_left: "[2024-01-01, 2025-01-01]"}}
```

---

### 9. LTree (Hierarchical Path) Operators

LTree operators work with PostgreSQL `ltree` type for hierarchical data (categories, org charts, etc.).

#### **Path Comparison**

| Operator | SQL Equivalent | Description | Example |
|----------|---|---|---|
| `eq` | `=` | Path equality | `{path: {eq: "Top.Sciences.Astronomy"}}` |
| `neq` | `!=` | Path inequality | `{path: {neq: "Other"}}` |
| `in` | `IN (...)` | Path in list | `{path: {in: ["Top.Science", "Top.Technology"]}}` |
| `nin` / `notin` | `NOT IN (...)` | Path not in list | `{path: {nin: ["Deleted"]}}` |

**Examples**:
```graphql
{path: {eq: "Organization.Engineering.Backend"}}
{path: {in: ["Products.Electronics", "Products.Software"]}}
```

#### **Path Hierarchy**

| Operator | SQL Equivalent | Description | Example |
|----------|---|---|---|
| `ancestor_of` | `@>` | Is ancestor of path | `{path: {ancestor_of: "Top.Sciences.Astronomy.Cosmology"}}` |
| `descendant_of` | `<@` | Is descendant of path | `{path: {descendant_of: "Top.Sciences"}}` |

**Hierarchy Rules**:
- `ancestor_of`: Current path is parent/ancestor of provided path
- `descendant_of`: Current path is child/descendant of provided path

**Examples**:
```graphql
# Find all ancestor categories
{path: {ancestor_of: "Top.Sciences.Physics.Quantum.Superposition"}}
# Matches: "Top", "Top.Sciences", "Top.Sciences.Physics", etc.

# Find all subcategories
{path: {descendant_of: "Top.Sciences"}}
# Matches: "Top.Sciences.Physics", "Top.Sciences.Astronomy", etc.

# Organization hierarchy
{path: {ancestor_of: "Company.Engineering.Backend.Database.Migration"}}
# Matches all parent org units
```

#### **Path Pattern Matching**

| Operator | SQL Equivalent | Description | Example |
|----------|---|---|---|
| `matches_lquery` | `~` | Matches lquery pattern | `{path: {matches_lquery: "Top.*.Ast*"}}` |
| `matches_ltxtquery` | `?` | Matches ltxtquery pattern | `{path: {matches_ltxtquery: "Top & (Sciences \| Technology)"}}` |
| `matches_any_lquery` | Array of patterns | Matches any lquery pattern | `{path: {matches_any_lquery: ["Top.*", "Other.*"]}}` |

**lquery Pattern Syntax** (SQL wildcards):
- `*` = Any level (single label)
- `*{n}` = Exactly n levels
- `*{n,}` = n or more levels
- `*{,n}` = Up to n levels
- `*{n,m}` = Between n and m levels
- `?[*]` = Optional level
- `!` = Prefix match (case-insensitive alternative)

**ltxtquery Syntax** (Boolean):
- `&` = AND
- `|` = OR
- `!` = NOT
- `(...)` = Grouping

**Examples**:
```graphql
# Find all paths with 3 levels starting with Top
{path: {matches_lquery: "Top.*.*"}}

# Find all paths in Sciences or Technology
{path: {matches_ltxtquery: "(Sciences | Technology)"}}

# Complex Boolean queries
{path: {matches_ltxtquery: "(Physics | Chemistry) & ! Deprecated"}}

# Match any pattern
{path: {matches_any_lquery: ["Products.*", "Services.*", "Other"]}}
```

#### **Path Depth/Navigation**

| Operator | SQL Equivalent | Description | Example |
|----------|---|---|---|
| `depth_eq` | `nlevel() =` | Depth equals | `{path: {depth_eq: 3}}` |
| `depth_neq` | `nlevel() !=` | Depth not equal | `{path: {depth_neq: 1}}` |
| `depth_gt` | `nlevel() >` | Depth greater | `{path: {depth_gt: 2}}` |
| `depth_gte` | `nlevel() >=` | Depth >= | `{path: {depth_gte: 2}}` |
| `depth_lt` | `nlevel() <` | Depth less | `{path: {depth_lt: 5}}` |
| `depth_lte` | `nlevel() <=` | Depth <= | `{path: {depth_lte: 4}}` |

**Depth Calculation**:
- `"Top"` = depth 1
- `"Top.Sciences"` = depth 2
- `"Top.Sciences.Physics"` = depth 3

**Examples**:
```graphql
# Top-level categories only
{path: {depth_eq: 1}}

# No deeply nested paths
{path: {depth_lt: 5}}

# Second-level categories
{path: {depth_eq: 2}}

# Deep hierarchies (3+ levels)
{path: {depth_gte: 3}}
```

#### **Path Concatenation**

| Operator | SQL Equivalent | Description | Example |
|----------|---|---|---|
| `concat` | `\|\|` | Concatenate paths | `{path: {concat: "Astronomy"}}` |

**Examples**:
```graphql
# Append label to path
{path: {concat: "NewSubcategory"}}
```

#### **Lowest Common Ancestor**

| Operator | SQL Equivalent | Description | Example |
|----------|---|---|---|
| `lca` | `lca()` | Lowest common ancestor | `{path: {lca: ["Top.Sciences.Astronomy", "Top.Sciences.Physics"]}}` |

**Examples**:
```graphql
# Find common ancestor
{path: {lca: ["Org.Engineering.Backend", "Org.Engineering.Frontend"]}}
# Returns: "Org.Engineering"
```

---

### 10. Vector Operators

Vector operators work with pgvector extension for semantic search and similarity matching.

#### **Vector Distance Operators**

| Operator | SQL Equivalent | Distance Metric | Use Case | Example |
|----------|---|---|---|---|
| `cosine_distance` | `<=>` | Cosine similarity (0=identical, 2=opposite) | **Default for embeddings** | `{embedding: {cosine_distance: {vector: [...], threshold: 0.1}}}` |
| `l2_distance` | `<->` | Euclidean (L2) distance | Spatial distance, clustering | `{embedding: {l2_distance: {vector: [...], threshold: 5.0}}}` |
| `l1_distance` | `<+>` | Manhattan (L1) distance | Grid-based distance | `{embedding: {l1_distance: {vector: [...], threshold: 10.0}}}` |
| `hamming_distance` | `<~>` | Hamming distance (binary vectors) | Bit similarity | `{bits: {hamming_distance: {vector: [...], threshold: 3}}}` |
| `jaccard_distance` | `<%>` | Jaccard distance (binary vectors) | Set similarity | `{bits: {jaccard_distance: {vector: [...], threshold: 0.5}}}` |

**Distance Methods Explained**:

**Cosine Distance** (recommended for embeddings):
- Measures angle between vectors (0 = identical, 1 = perpendicular, 2 = opposite)
- Dimension-invariant (works with any embedding size)
- Best for: Text embeddings, image embeddings, semantic search
- Range: [0, 2]
- Lower = more similar

**Euclidean Distance** (L2):
- Straight-line distance in multi-dimensional space
- Depends on vector magnitude and dimension
- Best for: Spatial data, clustering, RMS error
- Formula: sqrt(sum((a-b)²))

**Manhattan Distance** (L1):
- Sum of absolute differences
- Useful in high-dimensional spaces (curse of dimensionality)
- Best for: Categorical data with grid structure
- Formula: sum(|a-b|)

**Hamming Distance** (binary vectors):
- Count differing bits
- Best for: Binary vectors, bit flags
- Range: [0, n] where n = vector length

**Jaccard Distance** (binary vectors):
- Set overlap similarity
- Best for: Set membership, presence/absence

**Query Format**:
```graphql
{
  vector_field: {
    distance_operator: {
      vector: [dimension1, dimension2, ...],
      threshold: number_threshold,
      comparison: "lt" | "lte" | "gt" | "gte"  # How to compare
    }
  }
}
```

**Examples**:
```graphql
# Semantic search (find similar embeddings)
{
  embedding: {
    cosine_distance: {
      vector: [0.1, 0.2, 0.3, -0.1, 0.05],
      threshold: 0.2,
      comparison: "lt"  # Find embeddings with distance < 0.2
    }
  }
}

# Spatial search (find nearby points)
{
  location: {
    l2_distance: {
      vector: [40.7128, -74.0060],  # NYC coordinates
      threshold: 10,  # Within 10 units
      comparison: "lt"
    }
  }
}

# Find closest match
{
  embedding: {
    l2_distance: {
      vector: [reference_vector],
      threshold: 100,
      comparison: "lt"
    }
  }
}
```

---

### 11. Full-Text Search Operators

Full-text search operators work with PostgreSQL `tsvector` type for advanced text searching.

#### **Full-Text Query Types**

| Operator | Function | Description | Example |
|----------|---|---|---|
| `matches` | `tsquery` | Boolean full-text query | `{content: {matches: "search & query"}}` |
| `plain_query` | `plainto_tsquery` | Simple phrase (auto-split on whitespace) | `{content: {plain_query: "quick brown fox"}}` |
| `phrase_query` | `phraseto_tsquery` | Exact phrase (consecutive words) | `{content: {phrase_query: "quick brown"}}` |
| `websearch_query` | `websearch_to_tsquery` | Web search syntax (Google-like) | `{content: {websearch_query: '"exact phrase" -exclude'}}` |

**Boolean Query Syntax** (`matches`):
- `&` = AND (both must match)
- `|` = OR (either must match)
- `!` = NOT (negation)
- `(...)` = Grouping
- `:*` = Prefix match

**Websearch Syntax** (`websearch_query`):
- `"phrase"` = Exact phrase
- `-word` = Exclude word
- `word1 word2` = AND (both required)
- `word1 | word2` = OR

**Examples**:
```graphql
# Boolean query (AND)
{content: {matches: "database & search"}}

# Boolean query (OR)
{content: {matches: "python | java"}}

# Exclude terms
{content: {matches: "database & !nosql"}}

# Complex Boolean
{content: {matches: "(python | java) & (database & !mongodb)"}}

# Prefix match
{content: {matches: "graphql:*"}}

# Plain phrase (auto-split)
{content: {plain_query: "apollo federation gateway"}}

# Exact phrase (consecutive)
{content: {phrase_query: "apollo federation"}}

# Web search
{content: {websearch_query: '"exact phrase" -exclude-word"}}

# Web search OR
{content: {websearch_query: 'python | java'}}
```

#### **Full-Text Ranking**

| Operator | Function | Description | Example |
|----------|---|---|---|
| `rank_gt` | `ts_rank() >` | Rank greater than | `{content: {rank_gt: {query: "search", threshold: 0.5}}}` |
| `rank_lt` | `ts_rank() <` | Rank less than | `{content: {rank_lt: {query: "search", threshold: 0.1}}}` |
| `rank_cd_gt` | `ts_rank_cd() >` | Cover density rank > | `{content: {rank_cd_gt: "search:0.5"}}` |
| `rank_cd_lt` | `ts_rank_cd() <` | Cover density rank < | `{content: {rank_cd_lt: "search:0.1"}}` |

**Ranking Types**:
- `ts_rank()`: Standard ranking (0-1 scale)
  - Uses position and frequency
  - Faster, suitable for most cases
- `ts_rank_cd()`: Cover density ranking (0-1 scale)
  - Considers proximity of query terms
  - Better for phrase relevance
  - Slower but more accurate for phrases

**Score Interpretation**:
- 0 = No relevance
- 0-0.3 = Low relevance
- 0.3-0.7 = Medium relevance
- 0.7-1.0 = High relevance

**Examples**:
```graphql
# Only highly relevant results
{content: {rank_gt: {query: "machine learning", threshold: 0.7}}}

# Filter out low-relevance noise
{content: {rank_gt: {query: "data", threshold: 0.2}}}

# Cover density ranking (better for phrases)
{content: {rank_cd_gt: "artificial intelligence:0.5"}}
```

---

### 12. JSONB Operators

JSONB operators work with PostgreSQL `jsonb` columns for flexible, semi-structured data.

#### **JSONB Containment**

| Operator | SQL Equivalent | Description | Example |
|----------|---|---|---|
| `overlaps` | `&&` | JSONB objects/arrays overlap | `{metadata: {overlaps: {"key": "value"}}}` |
| `strictly_contains` | `@>` AND `!=` | Contains but not equal | `{metadata: {strictly_contains: {"key": "value"}}}` |

**Overlap Rules**:
- Objects: Share at least one key-value pair
- Arrays: Share at least one element

**Examples**:
```graphql
# Check if metadata contains required field
{metadata: {strictly_contains: {"environment": "production"}}}

# Check overlap between JSONB fields
{config: {overlaps: {"debug": true}}}
```

---

### 13. Coordinate/Geographic Operators

Geographic operators work with PostgreSQL `point` type for coordinate-based filtering.

#### **Coordinate Comparison**

| Operator | SQL Equivalent | Description | Example |
|----------|---|---|---|
| `eq` | `POINT = POINT` | Exact coordinate match | `{location: {eq: (40.7128, -74.0060)}}` |
| `neq` | `POINT != POINT` | Coordinate not equal | `{location: {neq: (0, 0)}}` |
| `in` | `IN (points)` | Coordinate in list | `{location: {in: [(40.7128, -74.0060), (51.5074, -0.1278)]}}` |
| `notin` | `NOT IN (points)` | Coordinate not in list | `{location: {notin: [(0, 0)]}}` |

**Format**: `(latitude, longitude)` tuples

**Examples**:
```graphql
# New York
{location: {eq: (40.7128, -74.0060)}}

# Major cities
{location: {in: [(40.7128, -74.0060), (51.5074, -0.1278), (48.8566, 2.3522)]}}

# Exclude null island
{location: {notin: [(0, 0)]}}
```

#### **Distance-Based Queries**

| Operator | Distance Method | Description | Example |
|----------|---|---|---|
| `distance_within` | Haversine / PostGIS / Earthdistance | Find within distance | `{location: {distance_within: ((40.7128, -74.0060), 5000)}}` |

**Distance Methods** (configurable):

| Method | Pros | Cons | Best For |
|--------|------|------|----------|
| **Haversine** (default) | No dependencies, fast | Assumes spherical Earth | General purpose, globe-scale |
| **PostGIS** | Most accurate, rich operators | Requires PostGIS | Advanced geospatial |
| **Earthdistance** | PostgreSQL native | Requires extension | Earth distances only |

**Distance Units**:
- **Haversine**: Kilometers by default (R = 6,371 km)
- **PostGIS**: Depends on SRID (usually meters)
- **Earthdistance**: Earth radii

**Format**: `((latitude, longitude), distance)`

**Examples**:
```graphql
# Find locations within 5 km of NYC
{location: {distance_within: ((40.7128, -74.0060), 5000)}}

# Within 1 degree (≈111 km at equator)
{location: {distance_within: ((0, 0), 1)}}

# Cities within 50 km of reference point
{location: {distance_within: ((48.8566, 2.3522), 50000)}}
```

---

### 14. Logical Operators

Logical operators combine multiple conditions.

#### **Logical Combinations**

| Operator | SQL Equivalent | Description | Example |
|----------|---|---|---|
| `AND` | `AND` | All conditions must match (default) | `{AND: [{status: {eq: "active"}}, {age: {gte: 18}}]}` |
| `OR` | `OR` | At least one condition must match | `{OR: [{status: {eq: "active"}}, {status: {eq: "pending"}}]}` |
| `NOT` | `NOT` | Negate condition | `{NOT: {status: {eq: "deleted"}}}` |

**Default Behavior**:
- Multiple conditions at same level are ANDed by default
- Explicit AND/OR for clarity and complex logic

**Examples**:
```graphql
# Implicit AND (default)
{
  status: {eq: "active"},
  age: {gte: 18}
}

# Explicit AND (more readable)
{
  AND: [
    {status: {eq: "active"}},
    {age: {gte: 18}},
    {country: {eq: "US"}}
  ]
}

# OR logic
{
  OR: [
    {status: {eq: "vip"}},
    {purchaseTotal: {gte: 10000}}
  ]
}

# NOT logic
{
  NOT: {status: {eq: "banned"}}
}

# Complex nested logic
{
  AND: [
    {age: {gte: 18}},
    {OR: [
      {status: {eq: "active"}},
      {status: {eq: "pending"}}
    ]},
    {NOT: {email: {isnull: true}}}
  ]
}

# De Morgan's Law example
# Not (A AND B) = (Not A) OR (Not B)
{
  OR: [
    {NOT: {age: {gte: 18}}},
    {NOT: {status: {eq: "active"}}}
  ]
}
```

---

### 15. Boolean Operators

Boolean operators work with boolean columns.

| Operator | Description | Example |
|----------|---|---|
| `eq` | Boolean equality | `{isActive: {eq: true}}` |
| `neq` | Boolean inequality | `{isActive: {neq: false}}` |
| `isnull` | Check if NULL | `{isActive: {isnull: false}}` |

**Examples**:
```graphql
# Active users
{isActive: {eq: true}}

# Non-deleted records
{isDeleted: {eq: false}}

# Required boolean fields
{isVerified: {isnull: false}}

# Inactive AND unverified
{AND: [{isActive: {eq: false}}, {isVerified: {eq: false}}]}
```

---

## Operator Combinations

Operators can be combined in complex queries:

### Complex Query Examples

```graphql
# Users aged 18-65, active, from specific countries
{
  AND: [
    {age: {gte: 18, lte: 65}},
    {status: {eq: "active"}},
    {country: {in: ["US", "CA", "MX"]}}
  ]
}

# Products in price range, with matching tags
{
  AND: [
    {price: {gte: 100, lte: 500}},
    {tags: {overlaps: ["featured", "bestseller"]}}
  ]
}

# Articles matching search OR by author
{
  OR: [
    {content: {matches: "postgresql & graphql"}},
    {author: {icontains: "Smith"}}
  ]
}

# Locations within distance with specific properties
{
  AND: [
    {location: {distance_within: ((40.7128, -74.0060), 10000)}},
    {type: {eq: "restaurant"}},
    {rating: {gte: 4.0}}
  ]
}

# Hierarchical filtering
{
  AND: [
    {category: {descendant_of: "Products.Electronics"}},
    {inStock: {eq: true}},
    {OR: [
      {discount: {gte: 20}},
      {price: {lt: 100}}
    ]}
  ]
}

# Full-text search with filters
{
  AND: [
    {content: {matches: "machine & learning"}},
    {createdAt: {gte: "2024-01-01"}},
    {NOT: {archived: {eq: true}}}
  ]
}
```

---

## Performance Considerations

### Indexing Recommendations

| Column Type | Recommended Indexes | Operators | Notes |
|----------|---|---|---|
| String | B-tree | contains, icontains, startswith, like | Pattern matching slower without indexes |
| Numeric | B-tree | gt, gte, lt, lte, between | Range queries benefit from B-tree |
| UUID | B-tree | eq, in | Fast lookups |
| INET/CIDR | GiST | insubnet, overlaps, isprivate | GiST indexes optimal |
| tsvector | GIN | matches, plain_query, phrase_query | Full-text search requires GIN |
| ltree | GiST | ancestor_of, descendant_of, matches_lquery | Hierarchical queries benefit from GiST |
| vector (pgvector) | IVFFlat or HNSW | cosine_distance, l2_distance | Vector indexes critical for performance |
| point | GiST | distance_within | Spatial indexes improve distance queries |
| daterange | GiST | overlaps, contains_date, adjacent | Range queries benefit from GiST |

### Query Optimization Tips

1. **Use indexed operators**: Operators matching indexes execute faster
2. **Combine filters**: AND multiple simple filters before OR
3. **Filter early**: Narrow results early to reduce downstream computation
4. **Use specific types**: Choose typed operators over generic fallbacks
5. **Limit vector searches**: Use tight thresholds to reduce result set
6. **Regular expression complexity**: Simple patterns faster than complex regex
7. **JSONB path indexes**: Consider indexes on frequently queried JSONB paths

---

## Error Handling

Invalid operator/type combinations return GraphQL errors:

```graphql
# Error: `matches` not supported for numeric fields
{age: {matches: "^2[0-9]$"}}
# GraphQL Error: "Operator 'matches' not supported for type 'integer'"

# Error: `distance_within` requires valid distance value
{location: {distance_within: (40.7128, -74.0060)}}
# GraphQL Error: "Operator 'distance_within' requires distance parameter"

# Error: `array_contains` for non-array field
{name: {contains: ["a", "b"]}}
# GraphQL Error: "Operator 'array_contains' not supported for type 'string'"
```

---

## Summary

FraiseQL's 150+ WHERE clause operators provide:

✅ **Type Safety**: Operators validated per column type
✅ **SQL Efficiency**: Direct translation to optimal SQL
✅ **Flexibility**: 15 categories covering all use cases
✅ **Readability**: Intuitive, chainable syntax
✅ **Performance**: Indexable operators, efficient execution
✅ **Specialized Features**: Vector search, full-text, hierarchies, geospatial

Whether filtering simple strings, searching text, querying hierarchies, or finding nearby coordinates, FraiseQL's operators provide efficient, type-safe solutions.
