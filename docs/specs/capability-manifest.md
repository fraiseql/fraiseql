<!-- Skip to main content -->
---

title: Capability Manifest Specification
description: The **Capability Manifest** is a machine-readable declaration of which WHERE operators each database supports. It drives compile-time schema generation, ensurin
keywords: ["format", "compliance", "protocol", "specification", "standard"]
tags: ["documentation", "reference"]
---

# Capability Manifest Specification

**Version:** 1.0
**Date:** January 11, 2026
**Status:** Complete
**Audience:** Compiler Engineers, Database Integration, Maintainers

---

## 1. Overview

The **Capability Manifest** is a machine-readable declaration of which WHERE operators each database supports. It drives compile-time schema generation, ensuring GraphQL schemas expose only operators the target database supports.

### Purpose

- **Database-specific operator availability** — Define what filtering operations each database can perform
- **Compile-time schema specialization** — Generate WHERE input types based on capabilities
- **Multi-database support** — Same schema source, different GraphQL APIs per database target
- **Deterministic validation** — Client queries are validated against supported operators at compile time
- **Extensibility** — New operators, new databases without runtime changes

### Core Principle

> **All operator availability is determined at compile time.** The Rust runtime executes compiled plans; it never interprets operator names or supports runtime fallbacks.

---

## 2. Manifest Structure

### 2.1 High-Level Format

```json
<!-- Code example in JSON -->
{
  "version": "1.0",
  "databases": {
    "postgresql": { ... },
    "mysql": { ... },
    "sql_server": { ... },
    "sqlite": { ... }
  }
}
```text
<!-- Code example in TEXT -->

### 2.2 Per-Database Structure

Each database entry declares operator support by **type category**:

```json
<!-- Code example in JSON -->
{
  "postgresql": {
    "identity": {
      "name": "PostgreSQL 15+",
      "version_constraint": ">=15.0",
      "vendor": "PostgreSQL"
    },
    "type_operators": {
      "String": [ ... ],
      "Int": [ ... ],
      "Float": [ ... ],
      "Boolean": [ ... ],
      "DateTime": [ ... ],
      "Date": [ ... ],
      "ID": [ ... ],
      "Decimal": [ ... ],
      "JSON": [ ... ],
      "JSONB": [ ... ],
      "Vector": [ ... ],
      "Network": [ ... ],
      "UUID": [ ... ],
      "Enum": [ ... ]
    },
    "capabilities": {
      "array_operators": true,
      "geographic_operators": true,
      "vector_operators": true,
      "jsonb_operators": true,
      "full_text_search": true
    }
  }
}
```text
<!-- Code example in TEXT -->

---

## 3. Operator Definitions

### 3.1 Operator Structure

Each operator is a simple string identifier:

```json
<!-- Code example in JSON -->
{
  "String": [
    "_eq",
    "_neq",
    "_like",
    "_ilike",
    "_regex",
    "_regex_icase",
    "_contains",
    "_contained_by",
    "_in",
    "_nin",
    "_is_null"
  ]
}
```text
<!-- Code example in TEXT -->

### 3.2 Standard Operators (All Databases)

These operators are supported by **every database** FraiseQL targets:

```json
<!-- Code example in JSON -->
"standard_operators": {
  "all_types": [
    "_eq",          // Equality
    "_neq",         // Not equal
    "_is_null",     // NULL check
    "_in",          // IN clause
    "_nin"          // NOT IN clause
  ],
  "comparable_types": [
    "_lt",          // Less than
    "_lte",         // Less than or equal
    "_gt",          // Greater than
    "_gte"          // Greater than or equal
  ],
  "string_types": [
    "_like",        // LIKE pattern (basic, case-sensitive)
    "_ilike",       // LIKE pattern (case-insensitive, PostgreSQL/MySQL)
    "_contains",    // String contains
    "_contained_by" // String contained by
  ]
}
```text
<!-- Code example in TEXT -->

### 3.3 Database-Specific Operators

#### PostgreSQL (Reference Implementation)

```json
<!-- Code example in JSON -->
{
  "postgresql": {
    "String": [
      // Standard
      "_eq", "_neq", "_like", "_ilike", "_contains", "_contained_by",
      "_in", "_nin", "_is_null",
      // PostgreSQL-specific
      "_regex",                 // ~ operator
      "_regex_icase",           // ~* operator
      "_starts_with",           // LIKE 'x%'
      "_ends_with",             // LIKE '%x'
      "_has_substring"          // POSITION
    ],
    "Int": [
      "_eq", "_neq", "_lt", "_lte", "_gt", "_gte",
      "_in", "_nin", "_is_null",
      "_between",               // BETWEEN operator
      "_bitwise_and",           // & operator
      "_bitwise_or"             // | operator
    ],
    "DateTime": [
      "_eq", "_neq", "_lt", "_lte", "_gt", "_gte",
      "_in", "_nin", "_is_null",
      "_between",
      "_extract_year",          // EXTRACT(YEAR FROM ...)
      "_extract_month",
      "_extract_day",
      "_extract_hour"
    ],
    "JSON": [
      "_eq", "_neq", "_is_null",
      "_jsonb_contains",        // @> operator
      "_jsonb_contained_by",    // <@ operator
      "_jsonb_has_key",         // ? operator
      "_jsonb_has_keys",        // ?| operator
      "_jsonb_has_all_keys",    // ?& operator
      "_jsonb_path_exists"      // @? operator
    ],
    "Vector": [
      "_eq",
      "_cosine_distance_lt",    // <-> operator
      "_l2_distance_lt",        // <-> operator (L2 norm)
      "_inner_product_gt",      // <#> operator
      "_cosine_similarity_gt"
    ],
    "Network": [
      "_eq", "_neq", "_is_null",
      "_cidr_contains",         // >> operator
      "_cidr_contained_by",     // << operator
      "_cidr_overlap",          // && operator
      "_inet_contains",
      "_inet_contained_by"
    ],
    "UUID": [
      "_eq", "_neq", "_in", "_nin", "_is_null"
    ]
  }
}
```text
<!-- Code example in TEXT -->

#### MySQL (Limited Operators)

```json
<!-- Code example in JSON -->
{
  "mysql": {
    "String": [
      "_eq", "_neq", "_like", "_in", "_nin", "_is_null",
      "_starts_with",
      "_ends_with",
      "_contains"
      // Note: No _ilike (case-insensitive), no _regex (not standard)
    ],
    "Int": [
      "_eq", "_neq", "_lt", "_lte", "_gt", "_gte",
      "_in", "_nin", "_is_null",
      "_between"
    ],
    "DateTime": [
      "_eq", "_neq", "_lt", "_lte", "_gt", "_gte",
      "_in", "_nin", "_is_null",
      "_between"
    ],
    "JSON": [
      "_eq", "_neq", "_is_null",
      "_json_extract",          // JSON_EXTRACT function
      "_json_contains",         // JSON_CONTAINS function
      "_json_length"            // JSON_LENGTH function
    ],
    "UUID": [
      "_eq", "_neq", "_in", "_nin", "_is_null"
    ]
  }
}
```text
<!-- Code example in TEXT -->

#### SQL Server (Moderate Operators)

```json
<!-- Code example in JSON -->
{
  "sql_server": {
    "String": [
      "_eq", "_neq", "_like", "_in", "_nin", "_is_null",
      "_contains",
      "_starts_with",
      "_ends_with"
      // Note: No _ilike, no _regex
    ],
    "Int": [
      "_eq", "_neq", "_lt", "_lte", "_gt", "_gte",
      "_in", "_nin", "_is_null",
      "_between"
    ],
    "DateTime": [
      "_eq", "_neq", "_lt", "_lte", "_gt", "_gte",
      "_in", "_nin", "_is_null",
      "_between"
    ],
    "JSON": [
      "_eq", "_neq", "_is_null",
      "_json_path",             // JSON_PATH_EXISTS
      "_json_query",            // JSON_QUERY function
      "_json_value"             // JSON_VALUE function
    ],
    "UUID": [
      "_eq", "_neq", "_in", "_nin", "_is_null"
    ]
  }
}
```text
<!-- Code example in TEXT -->

#### SQLite (Minimal Operators)

```json
<!-- Code example in JSON -->
{
  "sqlite": {
    "String": [
      "_eq", "_neq", "_like", "_in", "_nin", "_is_null"
      // Note: No _ilike, no _regex, no advanced string ops
    ],
    "Int": [
      "_eq", "_neq", "_lt", "_lte", "_gt", "_gte",
      "_in", "_nin", "_is_null"
    ],
    "DateTime": [
      "_eq", "_neq", "_lt", "_lte", "_gt", "_gte",
      "_in", "_nin", "_is_null"
    ],
    "JSON": [
      "_eq", "_neq", "_is_null"
      // Note: SQLite JSON support is minimal; no complex operators
    ],
    "UUID": [
      "_eq", "_neq", "_in", "_nin", "_is_null"
    ]
  }
}
```text
<!-- Code example in TEXT -->

### 3.4 Aggregation Operators

Aggregation operators are used in analytical queries (fact tables with `tf_*` prefix) for GROUP BY and HAVING clauses.

#### PostgreSQL Aggregation (Full Support)

```json
<!-- Code example in JSON -->
{
  "aggregation": {
    "basic": [
      {"function": "COUNT", "sql": "COUNT($1)", "return_type": "Int"},
      {"function": "COUNT_DISTINCT", "sql": "COUNT(DISTINCT $1)", "return_type": "Int"},
      {"function": "SUM", "sql": "SUM($1)", "return_type": "Numeric"},
      {"function": "AVG", "sql": "AVG($1)", "return_type": "Float"},
      {"function": "MIN", "sql": "MIN($1)", "return_type": "Same as input"},
      {"function": "MAX", "sql": "MAX($1)", "return_type": "Same as input"}
    ],
    "statistical": [
      {"function": "STDDEV", "sql": "STDDEV($1)", "return_type": "Float"},
      {"function": "VARIANCE", "sql": "VARIANCE($1)", "return_type": "Float"},
      {"function": "PERCENTILE_CONT", "sql": "PERCENTILE_CONT($1) WITHIN GROUP (ORDER BY $2)", "return_type": "Float"}
    ],
    "temporal_bucketing": {
      "function": "DATE_TRUNC",
      "sql": "DATE_TRUNC($1, $2)",
      "buckets": ["second", "minute", "hour", "day", "week", "month", "quarter", "year"]
    },
    "conditional": {
      "function": "FILTER",
      "sql": "$1 FILTER (WHERE $2)",
      "supported": true
    }
  }
}
```text
<!-- Code example in TEXT -->

#### MySQL Aggregation (Basic Support)

```json
<!-- Code example in JSON -->
{
  "aggregation": {
    "basic": [
      {"function": "COUNT", "sql": "COUNT($1)", "return_type": "Int"},
      {"function": "COUNT_DISTINCT", "sql": "COUNT(DISTINCT $1)", "return_type": "Int"},
      {"function": "SUM", "sql": "SUM($1)", "return_type": "Numeric"},
      {"function": "AVG", "sql": "AVG($1)", "return_type": "Float"},
      {"function": "MIN", "sql": "MIN($1)", "return_type": "Same as input"},
      {"function": "MAX", "sql": "MAX($1)", "return_type": "Same as input"}
    ],
    "statistical": [],
    "temporal_bucketing": {
      "function": "DATE_FORMAT",
      "sql": "DATE_FORMAT($1, $2)",
      "buckets": ["day", "week", "month", "year"]
    },
    "conditional": {
      "function": "FILTER",
      "sql": "CASE WHEN $2 THEN $1 ELSE 0 END",
      "supported": "emulated"
    }
  }
}
```text
<!-- Code example in TEXT -->

#### SQLite Aggregation (Minimal Support)

```json
<!-- Code example in JSON -->
{
  "aggregation": {
    "basic": [
      {"function": "COUNT", "sql": "COUNT($1)", "return_type": "Int"},
      {"function": "SUM", "sql": "SUM($1)", "return_type": "Numeric"},
      {"function": "AVG", "sql": "AVG($1)", "return_type": "Float"},
      {"function": "MIN", "sql": "MIN($1)", "return_type": "Same as input"},
      {"function": "MAX", "sql": "MAX($1)", "return_type": "Same as input"}
    ],
    "statistical": [],
    "temporal_bucketing": {
      "function": "strftime",
      "sql": "strftime($1, $2)",
      "buckets": ["day", "week", "month", "year"]
    },
    "conditional": {
      "function": "FILTER",
      "sql": "CASE WHEN $2 THEN $1 ELSE 0 END",
      "supported": "emulated"
    }
  }
}
```text
<!-- Code example in TEXT -->

#### SQL Server Aggregation (Enterprise Support)

```json
<!-- Code example in JSON -->
{
  "aggregation": {
    "basic": [
      {"function": "COUNT", "sql": "COUNT($1)", "return_type": "Int"},
      {"function": "COUNT_DISTINCT", "sql": "COUNT(DISTINCT $1)", "return_type": "Int"},
      {"function": "SUM", "sql": "SUM($1)", "return_type": "Numeric"},
      {"function": "AVG", "sql": "AVG($1)", "return_type": "Float"},
      {"function": "MIN", "sql": "MIN($1)", "return_type": "Same as input"},
      {"function": "MAX", "sql": "MAX($1)", "return_type": "Same as input"}
    ],
    "statistical": [
      {"function": "STDEV", "sql": "STDEV($1)", "return_type": "Float"},
      {"function": "STDEVP", "sql": "STDEVP($1)", "return_type": "Float"},
      {"function": "VAR", "sql": "VAR($1)", "return_type": "Float"},
      {"function": "VARP", "sql": "VARP($1)", "return_type": "Float"}
    ],
    "temporal_bucketing": {
      "function": "DATEPART",
      "sql": "DATEPART($1, $2)",
      "buckets": ["day", "week", "month", "quarter", "year", "hour", "minute"]
    },
    "conditional": {
      "function": "FILTER",
      "sql": "CASE WHEN $2 THEN $1 ELSE 0 END",
      "supported": "emulated"
    },
    "json": [
      {"function": "JSON_VALUE", "sql": "JSON_VALUE($1, $2)", "supported": true},
      {"function": "JSON_QUERY", "sql": "JSON_QUERY($1, $2)", "supported": true}
    ]
  }
}
```text
<!-- Code example in TEXT -->

**Related documentation**: See `docs/specs/aggregation-operators.md` for complete aggregation operator reference with examples.

---

## 4. Capability Flags

### 4.1 Feature Support

Each database can declare feature support:

```json
<!-- Code example in JSON -->
{
  "capabilities": {
    "array_operators": true,           // Array/list filtering
    "array_aggregation": true,         // array_agg(), GROUP_CONCAT()
    "geographic_operators": false,     // PostGIS, ST_* functions
    "vector_operators": false,         // pgvector or equivalent
    "full_text_search": false,         // FTS5, Sphinx, etc.
    "json_operators": true,            // JSON/JSONB filtering
    "computed_fields": true,           // Generated/computed columns
    "window_functions": true,          // OVER/PARTITION BY
    "cte_support": true,               // Common Table Expressions
    "recursive_cte": false,            // WITH RECURSIVE
    "composite_types": false,          // Structured types beyond JSON
    "enum_support": true,              // ENUM data type
    "custom_types": false,             // User-defined types
    "foreign_data_wrapper": false      // FDW (PostgreSQL)
  }
}
```text
<!-- Code example in TEXT -->

### 4.2 Feature-Gated Operators

Operators are only exposed if capability is true:

```yaml
<!-- Code example in YAML -->
# In compiler phase (WHERE type generation)
if capabilities.vector_operators:
  # PostgreSQL: Include vector distance operators
  include_operators("Vector", ["_cosine_distance_lt", "_l2_distance_lt", ...])
else:
  # MySQL: Skip vector operators (pgvector not available)
  skip_operators("Vector", ["_cosine_distance_lt"])
```text
<!-- Code example in TEXT -->

---

## 5. Operator Implementation Mapping

### 5.1 Operator-to-SQL Translation

Each operator maps to database-specific SQL:

```json
<!-- Code example in JSON -->
{
  "operator_mappings": {
    "postgresql": {
      "_eq": "= ?",
      "_neq": "!= ?",
      "_like": "LIKE ?",
      "_ilike": "ILIKE ?",
      "_regex": "~ ?",
      "_regex_icase": "~* ?",
      "_contains": "LIKE CONCAT('%', ?, '%')",
      "_jsonb_contains": "@> ?",
      "_cosine_distance_lt": "<-> ? < ?",
      "_lt": "< ?",
      "_lte": "<= ?",
      "_between": "BETWEEN ? AND ?",
      "_is_null": "IS NULL"
    },
    "mysql": {
      "_eq": "= ?",
      "_neq": "!= ?",
      "_like": "LIKE ?",
      "_ilike": "LIKE ? COLLATE utf8mb4_general_ci",
      "_contains": "LIKE CONCAT('%', ?, '%')",
      "_json_extract": "JSON_EXTRACT(?, ?) = ?",
      "_lt": "< ?",
      "_lte": "<= ?",
      "_between": "BETWEEN ? AND ?"
    },
    "sql_server": {
      "_eq": "= ?",
      "_neq": "!= ?",
      "_like": "LIKE ?",
      "_contains": "LIKE '%' + ? + '%'",
      "_json_path": "JSON_PATH_EXISTS(?, ?) = 1",
      "_lt": "< ?",
      "_lte": "<= ?",
      "_between": "BETWEEN ? AND ?"
    },
    "sqlite": {
      "_eq": "= ?",
      "_neq": "!= ?",
      "_like": "LIKE ?",
      "_contains": "LIKE '%' || ? || '%'",
      "_lt": "< ?",
      "_lte": "<= ?",
      "_in": "IN (...)"
    }
  }
}
```text
<!-- Code example in TEXT -->

---

## 6. Manifest Usage in Compilation

### 6.1 Compile-Time Phase: WHERE Type Generation

**Phase 4 of compilation pipeline** uses the manifest:

```python
<!-- Code example in Python -->
def generate_where_input_types(
    schema_types: List[Type],
    database_target: str,
    capability_manifest: Dict
) -> Dict[str, GraphQLInputType]:
    """
    Generate WHERE input types based on database capabilities.
    """
    capabilities = capability_manifest[database_target]
    where_types = {}

    for type_def in schema_types:
        for field in type_def.fields:
            field_type = field.type

            # Look up supported operators for this field type
            operators = capabilities['type_operators'].get(field_type, [])

            # Generate filter type with only supported operators
            filter_type = GraphQLInputType(
                name=f"{field.name}_FilterInput",
                fields={
                    op: GraphQLInputField(type=determine_op_arg_type(op))
                    for op in operators
                }
            )

            where_types[f"{type_def.name}_{field.name}_Filter"] = filter_type

    return where_types
```text
<!-- Code example in TEXT -->

### 6.2 Result: Database-Specific Schema

**PostgreSQL compilation:**

```graphql
<!-- Code example in GraphQL -->
input OrderWhereInput {
  id: IDFilter
  customer_id: IDFilter
  created_at: DateTimeFilter
  metadata: JSONBFilter      # ✅ PostgreSQL supports JSONB
}

input JSONBFilter {
  _eq: JSON
  _jsonb_contains: JSON
  _jsonb_has_key: String
  # ... 5 more JSONB operators ...
}
```text
<!-- Code example in TEXT -->

**MySQL compilation:**

```graphql
<!-- Code example in GraphQL -->
input OrderWhereInput {
  id: IDFilter
  customer_id: IDFilter
  created_at: DateTimeFilter
  metadata: JSONFilter       # ✅ MySQL, but fewer operators
}

input JSONFilter {
  _eq: JSON
  _json_extract: String
  _json_contains: JSON
  # ... no JSONB operators ...
}
```text
<!-- Code example in TEXT -->

---

## 7. Version Constraints

### 7.1 Database Version Targeting

The manifest supports version-specific operator availability:

```json
<!-- Code example in JSON -->
{
  "postgresql": {
    "identity": {
      "name": "PostgreSQL",
      "version_constraint": ">=15.0",
      "note": "Uses PostgreSQL 15+ syntax"
    }
  },
  "postgresql_14": {
    "identity": {
      "name": "PostgreSQL 14",
      "version_constraint": ">=14.0,<15.0",
      "note": "Limited operator set for v14"
    },
    "type_operators": {
      "Vector": []        // pgvector only in v15+
    }
  }
}
```text
<!-- Code example in TEXT -->

**Compilation Target:**

```yaml
<!-- Code example in YAML -->
# FraiseQL.yaml
database:
  type: postgresql
  version: "14.5"  # ← Selects postgresql_14 manifest entry
```text
<!-- Code example in TEXT -->

---

## 8. Validation Rules

### 8.1 Manifest Validation

FraiseQL validates manifests at load time:

✅ **Valid:**

```json
<!-- Code example in JSON -->
{
  "postgresql": {
    "String": ["_eq", "_neq", "_like"]
  }
}
```text
<!-- Code example in TEXT -->

❌ **Invalid (unknown operator):**

```json
<!-- Code example in JSON -->
{
  "postgresql": {
    "String": ["_eq", "_unknown_op"]
  }
}
```text
<!-- Code example in TEXT -->

Error: `Unknown operator: _unknown_op (did you mean _neq?)`

✅ **Compiler catches undefined operators:**

```graphql
<!-- Code example in GraphQL -->
query {
  orders(where: { customer_id: { _cosine_distance: 0.5 } }) {
    # ❌ ERROR: '_cosine_distance' not available for MySQL
    # Available: _eq, _neq, _in, _is_null
  }
}
```text
<!-- Code example in TEXT -->

---

## 9. Extending the Manifest

### 9.1 Adding a New Database

To support DuckDB:

```json
<!-- Code example in JSON -->
{
  "duckdb": {
    "identity": {
      "name": "DuckDB 0.9+",
      "version_constraint": ">=0.9.0"
    },
    "type_operators": {
      "String": [
        "_eq", "_neq", "_like", "_in", "_nin", "_is_null",
        "_contains", "_starts_with", "_ends_with", "_regex"
      ],
      "DateTime": [
        "_eq", "_neq", "_lt", "_lte", "_gt", "_gte",
        "_in", "_nin", "_is_null", "_between"
      ],
      "JSON": [
        "_eq", "_json_extract_path", "_json_extract"
      ]
    },
    "capabilities": {
      "array_operators": true,
      "json_operators": true,
      "full_text_search": false,
      "window_functions": true
    }
  }
}
```text
<!-- Code example in TEXT -->

### 9.2 Adding a New Operator

To add `_array_contains` for PostgreSQL:

```json
<!-- Code example in JSON -->
{
  "postgresql": {
    "Array": [
      "_eq",
      "_array_contains",      // NEW: Check if array contains element
      "_array_contained_by",
      "_array_overlap"
    ]
  }
}
```text
<!-- Code example in TEXT -->

The compiler automatically:

1. Recognizes the new operator
2. Generates GraphQL input field for it
3. Maps it to SQL (`@>` operator)
4. Includes it in compilation for PostgreSQL target

---

## 10. Reference Manifest

### Complete Entry: PostgreSQL 15+

```json
<!-- Code example in JSON -->
{
  "postgresql": {
    "identity": {
      "name": "PostgreSQL 15+",
      "version_constraint": ">=15.0",
      "vendor": "PostgreSQL",
      "recommended": true,
      "notes": "Reference implementation with full feature set"
    },
    "type_operators": {
      "String": [
        "_eq", "_neq", "_like", "_ilike", "_regex", "_regex_icase",
        "_contains", "_contained_by", "_starts_with", "_ends_with",
        "_in", "_nin", "_is_null"
      ],
      "Int": [
        "_eq", "_neq", "_lt", "_lte", "_gt", "_gte",
        "_in", "_nin", "_is_null", "_between",
        "_bitwise_and", "_bitwise_or", "_bitwise_xor"
      ],
      "Float": [
        "_eq", "_neq", "_lt", "_lte", "_gt", "_gte",
        "_in", "_nin", "_is_null", "_between"
      ],
      "Boolean": [
        "_eq", "_neq", "_is_null"
      ],
      "DateTime": [
        "_eq", "_neq", "_lt", "_lte", "_gt", "_gte",
        "_in", "_nin", "_is_null", "_between",
        "_extract_year", "_extract_month", "_extract_day",
        "_extract_hour", "_extract_minute", "_extract_second"
      ],
      "Date": [
        "_eq", "_neq", "_lt", "_lte", "_gt", "_gte",
        "_in", "_nin", "_is_null", "_between"
      ],
      "ID": [
        "_eq", "_neq", "_in", "_nin", "_is_null"
      ],
      "Decimal": [
        "_eq", "_neq", "_lt", "_lte", "_gt", "_gte",
        "_in", "_nin", "_is_null", "_between"
      ],
      "JSON": [
        "_eq", "_neq", "_is_null",
        "_jsonb_contains", "_jsonb_contained_by",
        "_jsonb_has_key", "_jsonb_has_keys",
        "_jsonb_path_exists"
      ],
      "Vector": [
        "_eq", "_cosine_distance_lt", "_l2_distance_lt",
        "_inner_product_gt", "_cosine_similarity_gt"
      ],
      "Network": [
        "_eq", "_neq", "_is_null",
        "_cidr_contains", "_cidr_contained_by", "_cidr_overlap",
        "_inet_contains", "_inet_contained_by"
      ],
      "UUID": [
        "_eq", "_neq", "_in", "_nin", "_is_null"
      ],
      "Enum": [
        "_eq", "_neq", "_in", "_nin", "_is_null"
      ]
    },
    "capabilities": {
      "array_operators": true,
      "array_aggregation": true,
      "geographic_operators": true,
      "vector_operators": true,
      "full_text_search": true,
      "json_operators": true,
      "computed_fields": true,
      "window_functions": true,
      "cte_support": true,
      "recursive_cte": true,
      "composite_types": true,
      "enum_support": true,
      "custom_types": true,
      "foreign_data_wrapper": true
    }
  }
}
```text
<!-- Code example in TEXT -->

---

## 11. FAQ

**Q: Can I add custom operators?**

A: Yes. Define them in the manifest entry for your database, and FraiseQL will automatically generate GraphQL fields for them.

**Q: What happens if I query an unsupported operator?**

A: Compile-time error. The operator won't exist in the generated GraphQL schema for that database target.

**Q: How do I add pgvector support?**

A: Add vector operators to the PostgreSQL capability manifest, and Vector type support. Compiler automatically includes them.

**Q: Does the manifest change at runtime?**

A: No. Manifests are compiled into the binary. All operator availability decisions happen at compile time.

---

**Status: Complete** — Capability manifest specification ready for implementation.
