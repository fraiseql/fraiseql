# LTree Path Operators Reference

FraiseQL provides comprehensive support for PostgreSQL's `ltree` data type, enabling powerful hierarchical path queries. This reference covers all path comparison operators, hierarchy operators, and path analysis functions.

## Overview

LTree is PostgreSQL's specialized data type for efficiently storing and querying hierarchical paths. It's optimized for tree structures like organizational hierarchies, file systems, category hierarchies, and geographic taxonomies.

## Quick Operator Reference

| Category | Operators | Use Case |
|----------|-----------|----------|
| **Path Comparison** | `eq`, `neq`, `lt`, `gt`, `lte`, `gte` | Compare paths lexicographically |
| **Hierarchy** | `ancestor_of`, `descendant_of` | Find parent/child relationships |
| **Pattern Matching** | `matches_lquery`, `matches_ltxtquery` | Query patterns with wildcards |
| **Path Depth** | `nlevel`, `nlevel_eq`, `nlevel_gt`, `nlevel_gte`, `nlevel_lt`, `nlevel_lte`, `nlevel_neq` | Filter by path depth |
| **Path Analysis** | `subpath`, `index`, `index_eq`, `index_gte` | Extract and find path components |
| **Path Manipulation** | `concat`, `lca` | Combine paths or find common ancestor |
| **Array Operations** | `in_array`, `array_contains`, `matches_any_lquery` | Work with arrays of paths |

---

## Path Comparison Operators

Path comparison operators use **lexicographic ordering** to compare paths as strings, where each label in the path is compared in order.

### Equality Operators

#### `eq` - Path Equals
**Use when:** Exact path match

```graphql
query {
  categories(where: {
    path: { eq: "top.science" }
  }) {
    id
    path
  }
}
```

**Behavior:**
- Exact string match after normalization
- PostgreSQL: `(path)::ltree = 'top.science'::ltree`
- Requires exact path (not substring)

---

#### `neq` - Path Not Equal
**Use when:** Exclude specific paths

```graphql
query {
  categories(where: {
    path: { neq: "archive.old" }
  }) {
    id
    path
  }
}
```

**SQL:** `(path)::ltree != 'archive.old'::ltree`

---

### Comparison Operators

#### `lt` - Path Less Than
**Use when:** Paths that come before another path lexicographically

```graphql
query {
  categories(where: {
    path: { lt: "middle.path" }
  }) {
    id
    path
  }
}
```

**Lexicographic ordering examples:**
- `"aaa" < "bbb"` → true
- `"top.alpha" < "top.beta"` → true
- `"top.a.z" < "top.b"` → true

**PostgreSQL:** `(path)::ltree < 'middle.path'::ltree`

---

#### `lte` - Path Less Than or Equal
**Use when:** Paths that come before or equal to another path

```graphql
query {
  categories(where: {
    path: { lte: "science.physics" }
  }) {
    id
    path
  }
}
```

**PostgreSQL:** `(path)::ltree <= 'science.physics'::ltree`

---

#### `gt` - Path Greater Than
**Use when:** Paths that come after another path lexicographically

```graphql
query {
  categories(where: {
    path: { gt: "middle.path" }
  }) {
    id
    path
  }
}
```

**Example paths that match `gt: "m"`:**
- `"n"`, `"z"`, `"top.xyz"` → true
- `"a"`, `"middle"` → false

**PostgreSQL:** `(path)::ltree > 'middle.path'::ltree`

---

#### `gte` - Path Greater Than or Equal
**Use when:** Paths that come after or equal to another path

```graphql
query {
  categories(where: {
    path: { gte: "science.physics" }
  }) {
    id
    path
  }
}
```

**PostgreSQL:** `(path)::ltree >= 'science.physics'::ltree`

---

## Hierarchy Operators

### `ancestor_of` - Is Ancestor
**Use when:** Find paths that contain another path

```graphql
query {
  categories(where: {
    path: { ancestorOf: "science.astronomy.nebulae" }
  }) {
    id
    path
  }
}
```

**Returns:** All ancestor paths of `"science.astronomy.nebulae"`
- `"science"`
- `"science.astronomy"`

**PostgreSQL:** `(path)::ltree @> 'science.astronomy.nebulae'::ltree`

---

### `descendant_of` - Is Descendant
**Use when:** Find paths that are children/descendants of another path

```graphql
query {
  categories(where: {
    path: { descendantOf: "science.astronomy" }
  }) {
    id
    path
  }
}
```

**Returns:** All descendant paths under `"science.astronomy"`
- `"science.astronomy.nebulae"`
- `"science.astronomy.planets"`
- `"science.astronomy.planets.terrestrial"`

**PostgreSQL:** `(path)::ltree <@ 'science.astronomy'::ltree`

---

## Pattern Matching Operators

### `matches_lquery` - Match LQuery Pattern
**Use when:** Wildcard patterns with optional/alternative labels

LQuery syntax allows:
- `*` - any single label
- `{n}` - exactly n levels
- `{n,}` - n or more levels
- `{n,m}` - between n and m levels
- `|` - alternation (a|b means "a" OR "b")

```graphql
query {
  categories(where: {
    path: { matchesLquery: "science.*" }
  }) {
    id
    path
  }
}
```

**Examples:**
- `"science.*"` → matches `science.astronomy`, `science.biology`, etc.
- `"*.*.astronomy"` → matches paths with exactly 3 levels ending in "astronomy"
- `"science.{2}"` → matches exactly 2 levels under "science"
- `"(astronomy|biology)"` → matches either "astronomy" or "biology"

**PostgreSQL:** `(path)::ltree ~ 'science.*'::lquery`

---

### `matches_ltxtquery` - Match LTxtQuery Pattern
**Use when:** Text search patterns with AND/OR/NOT logic

LTxtQuery syntax allows Boolean operators:
- `&` - AND
- `|` - OR
- `!` - NOT

```graphql
query {
  categories(where: {
    path: { matchesLtxtquery: "science & (astronomy | physics)" }
  }) {
    id
    path
  }
}
```

**Examples:**
- `"science & astronomy"` → both "science" AND "astronomy" present
- `"astronomy | geology"` → either "astronomy" OR "geology" present
- `"!archive"` → does NOT contain "archive"

**PostgreSQL:** `(path)::ltree ? 'science & astronomy'::ltxtquery`

---

## Path Depth Operators

### `nlevel` - Get Path Depth
**Use when:** You need the depth value (not filtering)

This returns the number of labels in the path:
- `"science"` → 1
- `"science.astronomy"` → 2
- `"science.astronomy.planets"` → 3

---

### Depth Comparison Operators

#### `nlevel_eq` / `depth_eq` - Exact Depth
**Use when:** Filter by exact depth

```graphql
query {
  categories(where: {
    path: { nlevelEq: 2 }
  }) {
    id
    path
  }
}
```

**Returns paths with exactly 2 labels:**
- `"science.astronomy"` → depth 2
- `"science.biology"` → depth 2
- `"science"` → depth 1 (not matched)

**PostgreSQL:** `nlevel((path)::ltree) = 2`

---

#### `nlevel_gt` / `depth_gt` - Depth Greater Than

```graphql
query {
  categories(where: {
    path: { nlevelGt: 2 }
  }) {
    id
    path
  }
}
```

**Returns paths with > 2 labels (3 or more)**

**PostgreSQL:** `nlevel((path)::ltree) > 2`

---

#### `nlevel_gte` / `depth_gte` - Depth Greater Than or Equal

```graphql
query {
  categories(where: {
    path: { nlevelGte: 3 }
  }) {
    id
    path
  }
}
```

**Returns paths with ≥ 3 labels**

---

#### `nlevel_lt` / `depth_lt` - Depth Less Than

```graphql
query {
  categories(where: {
    path: { nlevelLt: 3 }
  }) {
    id
    path
  }
}
```

**Returns paths with < 3 labels (1 or 2)**

---

#### `nlevel_lte` / `depth_lte` - Depth Less Than or Equal

```graphql
query {
  categories(where: {
    path: { nlevelLte: 3 }
  }) {
    id
    path
  }
}
```

**Returns paths with ≤ 3 labels**

---

#### `nlevel_neq` / `depth_neq` - Depth Not Equal

```graphql
query {
  categories(where: {
    path: { nlevelNeq: 2 }
  }) {
    id
    path
  }
}
```

**Returns paths that don't have exactly 2 labels**

---

## Path Analysis Operators

### `subpath` - Extract Subpath
**Use when:** Extract a portion of a path

```graphql
query {
  categories(where: {
    path: { subpath: [0, 2] }
  }) {
    id
    path
  }
}
```

**Parameters:** `[offset, length]`
- `offset` - starting position (0-indexed)
- `length` - number of labels to extract

**Examples:**
- `path: "science.astronomy.planets"` with `[0, 2]` → `"science.astronomy"`
- `path: "science.astronomy.planets"` with `[1, 2]` → `"astronomy.planets"`
- `path: "a.b.c.d"` with `[2, 1]` → `"c"`

**PostgreSQL:** `subpath((path)::ltree, 0, 2)`

---

### `index` - Find Label Index
**Use when:** Locate a specific label in a path

```graphql
query {
  categories(where: {
    path: { index: "astronomy" }
  }) {
    id
    path
  }
}
```

**Returns:** Position of label (0-indexed, or -1 if not found)

**Example:**
- `path: "science.astronomy.planets"` → index of `"astronomy"` is 1
- `path: "science.astronomy.planets"` → index of `"nonexistent"` is -1

---

### `index_eq` - Label at Position
**Use when:** Check if a specific label is at a position

```graphql
query {
  categories(where: {
    path: { indexEq: ["astronomy", 1] }
  }) {
    id
    path
  }
}
```

**Parameters:** `[label, position]`

Returns paths where `label` is at `position`.

---

### `index_gte` - Label at or After Position
**Use when:** Label appears at or after a position

```graphql
query {
  categories(where: {
    path: { indexGte: ["planets", 1] }
  }) {
    id
    path
  }
}
```

Returns paths where `"planets"` first appears at position 1 or later.

---

## Path Manipulation Operators

### `concat` - Concatenate Paths
**Use when:** Combine two paths

```graphql
query {
  categories(where: {
    path: { concat: "planets.terrestrial" }
  }) {
    id
    path
  }
}
```

**Example:**
- `path: "science.astronomy"` concatenated with `"planets.terrestrial"`
- Result: `"science.astronomy.planets.terrestrial"`

**PostgreSQL:** `(path)::ltree || 'planets.terrestrial'::ltree`

---

### `lca` - Lowest Common Ancestor
**Use when:** Find common ancestor path

```graphql
query {
  categories(where: {
    path: {
      lca: [
        "science.astronomy.planets",
        "science.astronomy.stars",
        "science.astronomy.nebulae"
      ]
    }
  }) {
    id
    path
  }
}
```

**Returns:** The lowest (deepest) common ancestor

**Example:**
- Input paths: `"science.astronomy.planets"`, `"science.astronomy.stars"`
- LCA result: `"science.astronomy"`

---

## Array Operations

### `in_array` - Path in Array
**Use when:** Check if path is in a list

```graphql
query {
  categories(where: {
    path: {
      inArray: [
        "science.astronomy",
        "science.biology",
        "science.physics"
      ]
    }
  }) {
    id
    path
  }
}
```

---

### `array_contains` - Array Contains Path
**Use when:** Array of paths contains a specific path

```graphql
query {
  categories(where: {
    paths: {
      arrayContains: "science.astronomy"
    }
  }) {
    id
    paths
  }
}
```

---

### `matches_any_lquery` - Match Any LQuery
**Use when:** Path matches any of multiple patterns

```graphql
query {
  categories(where: {
    path: {
      matchesAnyLquery: [
        "science.*",
        "history.*",
        "*.*.astronomy"
      ]
    }
  }) {
    id
    path
  }
}
```

---

## In Python

All operators work identically in Python dict-based queries:

```python
from fraiseql.sql import create_graphql_where_input, LTreeFilter

# Using WhereInput (type-safe)
where = CategoryWhereInput(
    path=LTreeFilter(
        eq="science.astronomy"
    )
)

# Using dict-based
where_dict = {
    "path": {
        "eq": "science.astronomy"
    }
}

# Path comparison
where_dict = {
    "path": {
        "gte": "science.astronomy",
        "lt": "science.biology"
    }
}

# Hierarchy
where_dict = {
    "path": {
        "descendantOf": "science.astronomy"
    }
}

# Depth filtering
where_dict = {
    "path": {
        "nlevelEq": 3
    }
}
```

---

## Performance Characteristics

### Index Support
All operators are **GiST index-optimized** in PostgreSQL:

```sql
-- Create GiST index for optimal performance
CREATE INDEX idx_path_gist ON categories USING GIST(path);
```

**Operator Performance:**

| Operator | Index Support | Notes |
|----------|---------------|-------|
| `eq`, `neq` | ✅ GiST | Exact match, very fast |
| `lt`, `lte`, `gt`, `gte` | ✅ GiST | Lexicographic comparison, fast |
| `ancestor_of`, `descendant_of` | ✅ GiST | Hierarchy queries, highly optimized |
| `matches_lquery`, `matches_ltxtquery` | ✅ GiST | Pattern matching, well-optimized |
| `nlevel_*` | ⚠️ B-tree only | Index on `nlevel(path)` for optimization |
| `subpath`, `index` | ⚠️ Function-based | Consider B-tree index on result |

### Query Optimization Tips

1. **Use GiST indexes** for best performance
   ```sql
   CREATE INDEX idx_category_path ON categories USING GIST(path);
   ```

2. **For depth-based queries**, create a functional index
   ```sql
   CREATE INDEX idx_category_depth ON categories(nlevel(path));
   ```

3. **Combine operators efficiently**
   ```graphql
   # Good: Ancestor_of alone uses GiST
   path: { ancestorOf: "science.astronomy" }

   # Better: Add depth restriction
   path: {
     ancestorOf: "science.astronomy"
   }
   depth: { nlevelGte: 3 }
   ```

---

## Real-World Examples

### Organizational Hierarchy
```graphql
query {
  departments(where: {
    path: {
      descendantOf: "engineering.backend"
    }
  }) {
    id
    path
    name
  }
}
```

### Category Tree with Depth Limit
```graphql
query {
  categories(where: {
    path: {
      descendantOf: "products.electronics"
      nlevelLte: 4  # Max 4 levels deep
    }
  }) {
    id
    path
  }
}
```

### Range Query on Paths
```graphql
query {
  categories(where: {
    path: {
      gte: "science"
      lt: "science.z"  # All science.* categories
    }
  }) {
    id
    path
  }
}
```

### Pattern Matching for Autocomplete
```graphql
query {
  categories(where: {
    path: {
      matchesLquery: "science.*"
    }
  }) {
    id
    path
  }
}
```

---

## Comparison with Other Operators

### Path Comparison vs Hierarchy
```graphql
# Path comparison: lexicographic order
path: { lt: "science.physics" }  # All paths before "science.physics"

# Hierarchy: ancestor/descendant relationship
path: { descendantOf: "science" }  # All paths under "science"
```

### Pattern Matching Operators
```graphql
# LQuery: wildcard patterns
matchesLquery: "science.*"  # Matches science.astronomy, science.biology, etc.

# LTxtQuery: Boolean patterns
matchesLtxtquery: "science & (astronomy | biology)"  # More complex logic
```

---

## See Also

- [LTree Type Guide](/docs/core/types-and-schema.md#ltree)
- [Filter Operators](/docs/advanced/filter-operators.md)
- [Where Clause Syntax](/docs/reference/where-clause-syntax-comparison.md)
- [PostgreSQL LTree Documentation](https://www.postgresql.org/docs/current/ltree.html)
