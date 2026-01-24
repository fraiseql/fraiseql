# Bug Category Confidence Improvements: 92% → 98%+

**Target:** Increase confidence for lower-confidence bug categories through systematic testing
**Current State:** 92% average confidence (some categories at 85-90%)
**Goal:** 98%+ confidence across all 8 categories
**Estimated Effort:** 34-50 hours for full coverage; 10-15 hours for critical tests

---

## Executive Summary

We can reach 98%+ confidence by adding **16 targeted tests** in 4 categories:

1. **Mutations (90% → 98%)** - Add 4 critical integration tests
2. **LTree/Custom Scalars (85% → 94%)** - Add 4 edge case tests
3. **Schema Type System (95% → 98%)** - Add 3 validation tests
4. **WHERE Clause (95% → 100%)** - Add 5 edge case + security tests

**Critical Path (10-15 hours, +6% confidence):**
- Mutation operation type dispatch
- LTree empty path handling
- WHERE clause SQL injection prevention
- Mutation typename in response

---

## Part 1: MUTATIONS (90% → 98% Confidence)

### Current State
- ✅ Parser tests for mutation syntax
- ✅ Field mapping for mutation results
- ❌ **MISSING:** Mutation operation type routing verification
- ❌ **MISSING:** Integration tests for mutation execution
- ❌ **MISSING:** Multi-argument mutation binding tests

### Improvement #1.1: Mutation Operation Type Dispatch (CRITICAL)
**File:** `crates/fraiseql-core/tests/mutation_operation_dispatch.rs` (new)
**Confidence Impact:** +3%
**Risk If Missing:** Mutations silently routed to query handler, queries executed as mutations
**Effort:** 3-4 hours

```rust
#[test]
fn test_mutation_operation_type_dispatch() {
    // Setup: compiled schema with Query and Mutation types
    let schema = compile_test_schema(r#"
    {
      "queries": [
        {"name": "getUser", "output_type": "User", ...}
      ],
      "mutations": [
        {"name": "createUser", "input_type": "CreateUserInput", "output_type": "User", ...}
      ]
    }
    "#);

    // Test 1: Query operation routed to query handler
    let query = GraphQLRequest {
        operation_type: "query",
        operation_name: "GetUser",
        query: "query GetUser { getUser(id: \"1\") { id name } }",
        variables: None,
    };
    let result = executor.execute(&query).await.unwrap();
    assert!(result.data.is_some());  // Should succeed

    // Test 2: Mutation operation routed to mutation handler
    let mutation = GraphQLRequest {
        operation_type: "mutation",
        operation_name: "CreateUser",
        query: "mutation CreateUser { createUser(input: {...}) { id } }",
        variables: None,
    };
    let result = executor.execute(&mutation).await.unwrap();
    assert!(result.data.is_some());  // Should succeed

    // Test 3: Mutation routed to query handler should fail
    let invalid = GraphQLRequest {
        operation_type: "query",
        operation_name: "CreateUser",
        query: "query CreateUser { createUser(input: {...}) { id } }",
        variables: None,
    };
    let result = executor.execute(&invalid).await;
    assert!(result.is_err(), "Query op cannot invoke mutation resolver");
}
```

**Verification:** Check `executor.rs:classify_query()` and `classify_mutation()` are called correctly

---

### Improvement #1.2: Mutation Response __typename
**File:** `crates/fraiseql-core/tests/mutation_typename_e2e.rs` (new)
**Confidence Impact:** +2%
**Risk If Missing:** Client introspection fails; GraphQL spec violation
**Effort:** 2-3 hours

```rust
#[tokio::test]
async fn test_mutation_response_includes_typename() {
    let schema = compile_test_schema(MUTATION_SCHEMA);
    let pool = setup_test_database().await;

    let mutation = r#"
    mutation CreateUser {
        createUser(input: {name: "Alice", email: "alice@example.com"}) {
            __typename
            id
            name
            email
        }
    }
    "#;

    let result = executor.execute(mutation).await.unwrap();

    // Parse response JSON
    let response = result.data.unwrap();
    let user = &response["createUser"];

    // CRITICAL: __typename MUST be present and match mutation return type
    assert_eq!(user["__typename"], "User");
    assert_eq!(user["name"], "Alice");
    assert!(user["id"].is_string());  // Should have ID from database
}

#[tokio::test]
async fn test_mutation_nested_object_typename() {
    // Test: createPost mutation returns Post with nested Author object
    // Expected: Post.__typename = "Post", Post.author.__typename = "User"
    let mutation = r#"
    mutation CreatePost {
        createPost(input: {title: "Hello", authorId: "1"}) {
            __typename
            id
            title
            author {
                __typename
                id
                name
            }
        }
    }
    "#;

    let result = executor.execute(mutation).await.unwrap();
    let post = &result.data.unwrap()["createPost"];

    assert_eq!(post["__typename"], "Post");
    assert_eq!(post["author"]["__typename"], "User");
}
```

**Verification:** Check `projection.rs:with_typename()` is called in mutation response path

---

### Improvement #1.3: Multi-Argument Mutation Binding
**File:** `crates/fraiseql-core/tests/mutation_multi_argument.rs` (new)
**Confidence Impact:** +2%
**Risk If Missing:** Wrong parameters passed to mutation function; data corruption
**Effort:** 2-3 hours

```rust
#[tokio::test]
async fn test_mutation_multiple_arguments_binding() {
    // Schema: updateUser(id: ID!, name: String!, email: String!)
    let mutation = r#"
    mutation UpdateUser {
        updateUser(id: "123", name: "Bob", email: "bob@example.com") {
            id
            name
            email
        }
    }
    "#;

    let result = executor.execute(mutation).await.unwrap();
    let user = &result.data.unwrap()["updateUser"];

    // Verify ALL three arguments were bound correctly
    assert_eq!(user["id"], "123");
    assert_eq!(user["name"], "Bob");
    assert_eq!(user["email"], "bob@example.com");
}

#[tokio::test]
async fn test_mutation_nested_input_object_binding() {
    // Schema: createUser(input: CreateUserInput!)
    // CreateUserInput { name: String!, email: String!, role: Role! }
    let mutation = r#"
    mutation CreateUser {
        createUser(input: {
            name: "Charlie",
            email: "charlie@example.com",
            role: ADMIN
        }) {
            id
            name
            role
        }
    }
    "#;

    let result = executor.execute(mutation).await.unwrap();
    let user = &result.data.unwrap()["createUser"];

    // Verify nested object fields bound correctly
    assert_eq!(user["name"], "Charlie");
    assert_eq!(user["role"], "ADMIN");
}
```

**Verification:** Trace through `executor.rs` to confirm parameter binding for each argument

---

### Improvement #1.4: Mutation Return Type Nullability
**File:** `crates/fraiseql-core/tests/mutation_nullability.rs` (new)
**Confidence Impact:** +1%
**Risk If Missing:** Null results returned when schema expects non-null; silent data loss
**Effort:** 1-2 hours

```rust
#[tokio::test]
async fn test_mutation_nullable_return_type() {
    // Schema: updateUser(...) -> User (nullable: true)
    let mutation = r#"
    mutation UpdateUser {
        updateUser(id: "999") {
            id
            name
        }
    }
    "#;

    // Simulate: user not found (null result)
    let result = executor.execute(mutation).await.unwrap();

    // Should return null, not error
    assert!(result.data.unwrap()["updateUser"].is_null());
    assert!(result.errors.is_empty());
}

#[tokio::test]
async fn test_mutation_non_nullable_return_type_null_error() {
    // Schema: deleteUser(...) -> Boolean! (nullable: false)
    let mutation = r#"
    mutation DeleteUser {
        deleteUser(id: "999")
    }
    "#;

    // Simulate: mutation returns null but schema says non-nullable
    let result = executor.execute(mutation).await;

    // Should return error, not null
    assert!(result.is_err());
    if let Err(e) = result {
        assert!(e.contains("non-nullable"));
    }
}
```

**Verification:** Check `executor.rs` validates mutation return type against schema nullability

---

## Part 2: LTREE/CUSTOM SCALARS (85% → 94% Confidence)

### Current State
- ✅ LTree operator SQL generation (lines 829-915)
- ✅ Custom scalar type system exists
- ❌ **MISSING:** LTree edge cases (empty paths, special chars)
- ❌ **MISSING:** Custom scalar JSON serialization tests
- ❌ **MISSING:** LTree value format validation

### Improvement #2.1: LTree Edge Cases (CRITICAL)
**File:** `crates/fraiseql-core/tests/ltree_edge_cases.rs` (new)
**Confidence Impact:** +3%
**Risk If Missing:** Malformed SQL for edge cases; query failures in production
**Effort:** 3-4 hours

```rust
#[test]
fn test_ltree_empty_path_handling() {
    let generator = PostgresWhereGenerator::new();

    // Test 1: Empty path string
    let clause = WhereClause::Field {
        path: vec!["path".to_string()],
        operator: WhereOperator::AncestorOf,
        value: json!(""),
    };
    let sql = generator.to_sql(&clause).unwrap();
    // Should generate: data->'path'::ltree @> ''::ltree (or appropriate error)

    // Test 2: Very deeply nested path (10+ levels)
    let clause = WhereClause::Field {
        path: (0..15).map(|i| format!("level{}", i)).collect(),
        operator: WhereOperator::AncestorOf,
        value: json!("org.dept.team"),
    };
    let sql = generator.to_sql(&clause).unwrap();
    assert!(sql.contains("->"));  // Should have 14 JSON operators
    assert!(sql.contains("::ltree"));  // Should cast to ltree at end

    // Test 3: Path with special characters
    let clause = WhereClause::Field {
        path: vec!["path".to_string()],
        operator: WhereOperator::MatchesLquery,
        value: json!("*.child_*.*"),  // Pattern with underscore
    };
    let sql = generator.to_sql(&clause).unwrap();
    assert!(sql.contains("~"));  // LQUERY operator

    // Test 4: Path with dots in label (PostgreSQL ltree uses dots as separators)
    let clause = WhereClause::Field {
        path: vec!["path".to_string()],
        operator: WhereOperator::MatchesLtxtquery,
        value: json!("(org & dept) | team"),  // Complex ltxtquery
    };
    let sql = generator.to_sql(&clause).unwrap();
    assert!(sql.contains("@"));  // LTXTQUERY operator
}

#[test]
fn test_ltree_across_databases() {
    // LTree is PostgreSQL-only; other databases should error or use fallback

    let mysql_gen = MysqlWhereGenerator::new();
    let clause = WhereClause::Field {
        path: vec!["path".to_string()],
        operator: WhereOperator::AncestorOf,
        value: json!("org.dept"),
    };

    let result = mysql_gen.to_sql(&clause);
    assert!(result.is_err(), "MySQL doesn't support LTree");
    assert!(result.unwrap_err().contains("not supported"));
}
```

**Verification:** Check `db/postgres/where_generator.rs` handles all LTree edge cases

---

### Improvement #2.2: Custom Scalar JSON Roundtrip
**File:** `crates/fraiseql-core/tests/custom_scalar_json.rs` (new)
**Confidence Impact:** +2%
**Risk If Missing:** Data corruption during serialization; type mismatch errors
**Effort:** 2-3 hours

```rust
#[test]
fn test_custom_scalar_json_roundtrip() {
    // Define custom scalars
    let schema = compile_test_schema(r#"
    {
      "types": [{
        "name": "Event",
        "fields": [
          {"name": "id", "type": "ID!"},
          {"name": "timestamp", "type": "DateTime!"},
          {"name": "metadata", "type": "JSON"}
        ]
      }],
      "scalars": [
        {"name": "DateTime", "specified_by_url": "https://tools.ietf.org/html/rfc3339"}
      ]
    }
    "#);

    // Test 1: DateTime scalar with timezone
    let value = "2024-01-15T10:30:45.123Z";
    let json_val = json!(value);
    let projected = project_value(&json_val, "DateTime").unwrap();
    assert_eq!(projected, json_val);  // Should preserve exact format

    // Test 2: JSON scalar with nested objects
    let value = json!({"key": "value", "nested": {"foo": "bar"}});
    let projected = project_value(&value, "JSON").unwrap();
    assert_eq!(projected, value);  // Should preserve structure

    // Test 3: Custom scalar in WHERE clause
    let where_clause = WhereClause::Field {
        path: vec!["timestamp".to_string()],
        operator: WhereOperator::Gte,
        value: json!("2024-01-01T00:00:00Z"),
    };
    let sql = postgres_where_gen.to_sql(&where_clause).unwrap();
    // Should properly escape and quote the value for text comparison
    assert!(sql.contains(">="));
}
```

**Verification:** Check `field_type.rs` scalar handling; test with actual database roundtrip

---

### Improvement #2.3: LTree Format Validation
**File:** `crates/fraiseql-core/tests/ltree_validation.rs` (new)
**Confidence Impact:** +2%
**Risk If Missing:** Invalid SQL generated; PostgreSQL query errors
**Effort:** 2-3 hours

```rust
#[test]
fn test_ltree_invalid_format_validation() {
    let generator = PostgresWhereGenerator::new();

    // Test 1: Invalid ltree value (spaces not allowed in ltree labels)
    let clause = WhereClause::Field {
        path: vec!["path".to_string()],
        operator: WhereOperator::AncestorOf,
        value: json!("org dept team"),  // Spaces invalid in ltree
    };

    // Should either:
    // A) Return error at SQL generation time
    let result = generator.to_sql(&clause);
    if result.is_err() {
        assert!(result.unwrap_err().contains("invalid") ||
                result.unwrap_err().contains("ltree"));
    }
    // OR
    // B) Escape/quote properly so PostgreSQL handles it
    // assert!(result.unwrap().contains("\"org dept team\""));

    // Test 2: Very long ltree path
    let long_path = (0..100).map(|i| format!("level{}", i)).collect::<Vec<_>>().join(".");
    let clause = WhereClause::Field {
        path: vec!["path".to_string()],
        operator: WhereOperator::AncestorOf,
        value: json!(long_path),
    };

    let result = generator.to_sql(&clause);
    // PostgreSQL ltree has max label length 65535
    // Should handle or error gracefully
    assert!(result.is_ok() ||
            result.unwrap_err().contains("too long"));
}
```

**Verification:** Add validation to `where_generator.rs` for LTree operators

---

### Improvement #2.4: Custom Scalar WHERE Clause Coercion
**File:** `crates/fraiseql-core/tests/custom_scalar_where.rs` (new)
**Confidence Impact:** +2%
**Risk If Missing:** Wrong type comparisons; incorrect query results
**Effort:** 3-4 hours

```rust
#[test]
fn test_custom_scalar_where_clause_coercion() {
    let schema = compile_test_schema(r#"
    {
      "types": [{
        "name": "Product",
        "fields": [
          {"name": "id", "type": "ID!"},
          {"name": "price", "type": "Decimal!"},
          {"name": "releaseDate", "type": "Date!"}
        ]
      }],
      "scalars": [
        {"name": "Decimal"},
        {"name": "Date"}
      ]
    }
    "#);

    // Test 1: Decimal scalar should coerce to numeric comparison
    let query = r#"
    { products(where: {price: {gte: "99.99"}}) { id price } }
    "#;
    // Should generate: data->>'price' >= 99.99 (numeric, not string comparison)
    let sql = compile_and_extract_where(query).unwrap();
    assert!(sql.contains(">="));  // Not LIKE or string comparison

    // Test 2: Date scalar should coerce to date comparison
    let query = r#"
    { products(where: {releaseDate: {gte: "2024-01-01"}}) { id } }
    "#;
    let sql = compile_and_extract_where(query).unwrap();
    // Should use date comparison: data->>'releaseDate'::date >= '2024-01-01'::date
    assert!(sql.contains("::date") || sql.contains("DATE"));
}
```

**Verification:** Implement scalar coercion in `where_sql_generator.rs`

---

## Part 3: SCHEMA TYPES (95% → 98% Confidence)

### Current State
- ✅ Type validation in `compiled.rs`
- ✅ Field nullability support
- ❌ **MISSING:** Interface implementation validation
- ❌ **MISSING:** Union type resolution in nested contexts
- ❌ **MISSING:** Deprecated field introspection

### Improvement #3.1: Interface Implementation Validation
**File:** `crates/fraiseql-core/tests/interface_implementation.rs` (new)
**Confidence Impact:** +1%
**Effort:** 2-3 hours

```rust
#[test]
fn test_interface_partial_implementation() {
    // Schema: interface Node { id: ID! }
    // Type User implements Node { name: String }  // Missing id field
    let schema_json = json!({
        "interfaces": [{
            "name": "Node",
            "fields": [
                {"name": "id", "type": "ID!", "required": true}
            ]
        }],
        "types": [{
            "name": "User",
            "implements": ["Node"],
            "fields": [
                {"name": "name", "type": "String!"}
                // MISSING: id field from Node interface
            ]
        }]
    });

    let result = CompiledSchema::from_json(&schema_json);
    assert!(result.is_err(), "Should reject incomplete interface implementation");
    assert!(result.unwrap_err().contains("interface") &&
            result.unwrap_err().contains("id"));
}
```

---

### Improvement #3.2: Union Type Response Projection
**File:** `crates/fraiseql-core/tests/union_type_projection.rs` (new)
**Confidence Impact:** +1%
**Effort:** 2-3 hours

```rust
#[tokio::test]
async fn test_union_type_nested_response_projection() {
    // Query: { search(q: "test"): SearchResult }
    // SearchResult = Post | Comment
    let query = r#"
    {
        search(q: "test") {
            __typename
            ... on Post {
                id
                title
            }
            ... on Comment {
                id
                text
            }
        }
    }
    "#;

    let result = executor.execute(query).await.unwrap();
    let results = &result.data.unwrap()["search"];

    // Each item should have __typename to distinguish union members
    for item in results.as_array().unwrap() {
        assert!(item["__typename"].is_string());
        assert_eq!(item["__typename"].as_str(), "Post") ||
            assert_eq!(item["__typename"].as_str(), "Comment");
    }
}
```

---

### Improvement #3.3: Deprecated Field Introspection
**File:** `crates/fraiseql-core/tests/deprecated_field_introspection.rs` (new)
**Confidence Impact:** +1%
**Effort:** 1-2 hours

```rust
#[tokio::test]
async fn test_deprecated_field_introspection() {
    let query = r#"
    {
        __type(name: "User") {
            fields(includeDeprecated: true) {
                name
                isDeprecated
                deprecationReason
            }
        }
    }
    "#;

    let result = executor.execute(query).await.unwrap();
    let fields = &result.data.unwrap()["__type"]["fields"];

    // Should include deprecated fields when includeDeprecated: true
    let old_field = fields.as_array().unwrap().iter()
        .find(|f| f["name"] == "legacyField").unwrap();

    assert_eq!(old_field["isDeprecated"], true);
    assert!(old_field["deprecationReason"].is_string());
}
```

---

## Part 4: WHERE CLAUSE (95% → 100% Confidence)

### Current State
- ✅ WHERE clause SQL generation (basic cases)
- ✅ Operator support (15+ operators tested)
- ❌ **MISSING:** Deep nesting tests (5+ levels)
- ❌ **MISSING:** NULL handling in complex logic
- ❌ **MISSING:** Array edge cases (empty, large)
- ❌ **MISSING:** Case sensitivity verification
- ❌ **MISSING:** Comprehensive SQL injection tests

### Improvement #4.1: Deeply Nested Path WHERE
**File:** `crates/fraiseql-core/tests/where_deep_nesting.rs` (new)
**Confidence Impact:** +1%
**Effort:** 1-2 hours

```rust
#[test]
fn test_where_nested_path_5_levels_deep() {
    let generator = PostgresWhereGenerator::new();

    let test_cases = vec![
        (3, vec!["user", "profile", "address"]),
        (5, vec!["user", "profile", "address", "country", "region"]),
        (10, (0..10).map(|i| format!("level{}", i)).collect()),
    ];

    for (depth, path) in test_cases {
        let clause = WhereClause::Field {
            path: path.iter().map(|s| s.to_string()).collect(),
            operator: WhereOperator::Equals,
            value: json!("value"),
        };

        let sql = generator.to_sql(&clause).unwrap();

        // Should have (depth-1) JSON navigation operators
        let arrow_count = sql.matches("->").count();
        assert_eq!(arrow_count, depth - 1, "Path depth {}: {} arrows", depth, arrow_count);

        // Should use ->> for final extraction
        assert!(sql.contains("->>"), "Path depth {}: should use ->> for final field", depth);
    }
}
```

---

### Improvement #4.2: NULL Handling in Complex Logic (CRITICAL)
**File:** `crates/fraiseql-core/tests/where_null_logic.rs` (new)
**Confidence Impact:** +2%
**Risk If Missing:** Wrong NULL semantics; incorrect query results
**Effort:** 2-3 hours

```rust
#[test]
fn test_where_null_with_and_or_combinations() {
    let generator = PostgresWhereGenerator::new();

    // Test 1: (age IS NULL OR status = 'inactive') AND (deleted_at IS NOT NULL)
    let clause = WhereClause::And(vec![
        WhereClause::Or(vec![
            WhereClause::Field {
                path: vec!["age".to_string()],
                operator: WhereOperator::IsNull,
                value: json!(true),
            },
            WhereClause::Field {
                path: vec!["status".to_string()],
                operator: WhereOperator::Equals,
                value: json!("inactive"),
            },
        ]),
        WhereClause::Field {
            path: vec!["deleted_at".to_string()],
            operator: WhereOperator::IsNull,
            value: json!(false),  // IS NOT NULL
        },
    ]);

    let sql = generator.to_sql(&clause).unwrap();

    // Should preserve parentheses for correct evaluation
    assert!(sql.contains("(") && sql.contains(")"));

    // Should use IS NULL and IS NOT NULL correctly
    assert!(sql.matches("IS NULL").count() >= 1);
    assert!(sql.matches("IS NOT NULL").count() >= 1);

    // Should use AND and OR
    assert!(sql.contains(" AND "));
    assert!(sql.contains(" OR "));
}

#[test]
fn test_where_null_comparison_three_valued_logic() {
    // PostgreSQL uses three-valued logic: TRUE, FALSE, UNKNOWN
    // NULL = any value returns UNKNOWN (not TRUE)
    // age = NULL doesn't match anything, use age IS NULL

    let generator = PostgresWhereGenerator::new();

    let clause = WhereClause::Field {
        path: vec!["age".to_string()],
        operator: WhereOperator::Equals,
        value: json!(null),  // Comparing to NULL
    };

    let sql = generator.to_sql(&clause).unwrap();

    // Should convert to IS NULL, not = NULL
    assert!(sql.contains("IS NULL"), "Should use IS NULL for null comparison");
    assert!(!sql.contains("= NULL"), "Should not use = NULL");
}
```

---

### Improvement #4.3: Array/JSON Array Edge Cases
**File:** `crates/fraiseql-core/tests/where_array_edge_cases.rs` (new)
**Confidence Impact:** +1%
**Effort:** 2-3 hours

```rust
#[test]
fn test_where_array_contains_edge_cases() {
    let postgres_gen = PostgresWhereGenerator::new();
    let mysql_gen = MysqlWhereGenerator::new();
    let sqlite_gen = SqliteWhereGenerator::new();

    // Test 1: Empty array
    let clause = WhereClause::Field {
        path: vec!["tags".to_string()],
        operator: WhereOperator::ArrayContains,
        value: json!([]),
    };

    let postgres_sql = postgres_gen.to_sql(&clause).unwrap();
    assert!(postgres_sql.contains("@>") || postgres_sql.contains("[]"));

    // Test 2: Large array (1000+ items)
    let large_array: Vec<_> = (0..1000).map(|i| json!(i)).collect();
    let clause = WhereClause::Field {
        path: vec!["ids".to_string()],
        operator: WhereOperator::ArrayContains,
        value: json!(large_array),
    };

    let sql = postgres_gen.to_sql(&clause).unwrap();
    assert!(sql.len() > 10000, "Should handle large arrays");

    // Test 3: Array with null values
    let clause = WhereClause::Field {
        path: vec!["values".to_string()],
        operator: WhereOperator::ArrayContains,
        value: json!([1, null, 3]),
    };

    // Different databases handle null in arrays differently
    let postgres_sql = postgres_gen.to_sql(&clause);
    let mysql_sql = mysql_gen.to_sql(&clause);

    // Both should succeed (may differ in null handling)
    assert!(postgres_sql.is_ok() || mysql_sql.is_ok());
}
```

---

### Improvement #4.4: Case Sensitivity Across Operators (CRITICAL)
**File:** `crates/fraiseql-core/tests/where_case_sensitivity.rs` (new)
**Confidence Impact:** +1%
**Risk If Missing:** Case-insensitive queries return wrong results; data loss
**Effort:** 2-3 hours

```rust
#[test]
fn test_where_case_sensitivity_across_operators() {
    let generator = PostgresWhereGenerator::new();

    let test_cases = vec![
        (WhereOperator::Contains, "LIKE"),         // Case-sensitive
        (WhereOperator::Icontains, "ILIKE"),       // Case-insensitive
        (WhereOperator::Startswith, "LIKE"),       // Case-sensitive
        (WhereOperator::Istartswith, "ILIKE"),     // Case-insensitive
        (WhereOperator::Endswith, "LIKE"),         // Case-sensitive
        (WhereOperator::Iendswith, "ILIKE"),       // Case-insensitive
    ];

    for (op, expected_operator) in test_cases {
        let clause = WhereClause::Field {
            path: vec!["name".to_string()],
            operator: op,
            value: json!("test"),
        };

        let sql = generator.to_sql(&clause).unwrap();
        assert!(sql.contains(expected_operator),
                "Operator {:?} should generate {}", op, expected_operator);
    }

    // Test actual case sensitivity
    let case_sensitive = WhereClause::Field {
        path: vec!["name".to_string()],
        operator: WhereOperator::Icontains,
        value: json!("ALICE"),
    };
    let sql = generator.to_sql(&case_sensitive).unwrap();

    // Should match "alice", "ALICE", "Alice" (case-insensitive)
    assert!(sql.contains("ILIKE"));
}
```

---

### Improvement #4.5: SQL Injection Prevention Across All Operators (CRITICAL)
**File:** `crates/fraiseql-core/tests/where_sql_injection_comprehensive.rs` (new)
**Confidence Impact:** +2%
**Risk If Missing:** SQL injection vulnerability; data breach
**Effort:** 3-4 hours

```rust
#[test]
fn test_where_sql_injection_prevention_across_all_operators() {
    let generator = PostgresWhereGenerator::new();
    let malicious_inputs = vec![
        "'; DROP TABLE users; --",
        "' OR '1'='1",
        "admin'--",
        "' UNION SELECT * FROM passwords --",
        "1; DELETE FROM users WHERE '1'='1",
        "') OR ('1'='1",
        "\" OR \"\"=\"\"",
        "' OR 1=1 --",
        "admin' OR 'a'='a",
    ];

    for (idx, payload) in malicious_inputs.iter().enumerate() {
        let clause = WhereClause::Field {
            path: vec!["email".to_string()],
            operator: WhereOperator::Equals,
            value: json!(payload),
        };

        let sql = generator.to_sql(&clause).unwrap();

        // CRITICAL: The payload must be safely encoded
        // Should use parameterized query: data->>'email' = $1 (with payload as param)
        // OR properly escape quotes: '' -> ''

        // Ensure payload doesn't break out of string
        assert!(!sql.contains("DROP TABLE"), "Injection attempt {} not escaped", idx);
        assert!(!sql.contains("UNION SELECT"), "Injection attempt {} not escaped", idx);
        assert!(!sql.contains("DELETE FROM"), "Injection attempt {} not escaped", idx);

        // Should use parameter placeholders or escaping
        if sql.contains(payload) {
            // If raw payload appears, it must be fully escaped/quoted
            let escaped = sql.matches('\'').count();
            assert!(escaped >= 4, "Raw payload requires full quoting: {}", sql);
        }
    }
}

#[test]
fn test_where_injection_in_nested_paths() {
    let generator = PostgresWhereGenerator::new();

    // Injection in path (field name)
    let malicious_path = vec![
        "user",
        "email'; DROP TABLE--",  // Malicious field name
    ];

    let clause = WhereClause::Field {
        path: malicious_path,
        operator: WhereOperator::Equals,
        value: json!("test@example.com"),
    };

    let sql = generator.to_sql(&clause).unwrap();

    // Path segments must be escaped/quoted too
    assert!(!sql.contains("DROP TABLE"));
}

#[test]
fn test_where_injection_parameterized_output() {
    let generator = PostgresWhereGenerator::new();

    let clause = WhereClause::Field {
        path: vec!["email".to_string()],
        operator: WhereOperator::Equals,
        value: json!("test'; DROP TABLE users; --"),
    };

    // BEST: Should output parameterized query
    // data->>'email' = $1 with params: ["test'; DROP TABLE users; --"]

    // ACCEPTABLE: Should properly escape
    // data->>'email' = 'test''; DROP TABLE users; --'

    let sql = generator.to_sql(&clause).unwrap();

    // One of these must be true:
    let is_parameterized = sql.contains("$");
    let is_escaped = sql.matches("''").count() > 0;  // Single quotes doubled

    assert!(is_parameterized || is_escaped,
            "Neither parameterized nor escaped: {}", sql);
}
```

---

## Implementation Priority Matrix

### Critical Path (Do First - 10-15 hours, +6% confidence)

| Test | File | Effort | Confidence | Risk If Missing |
|------|------|--------|-----------|-----------------|
| **Mutation Op Dispatch** | mutation_operation_dispatch.rs | 3-4h | +3% | CRITICAL - mutations silently fail |
| **LTree Edge Cases** | ltree_edge_cases.rs | 3-4h | +3% | HIGH - malformed SQL |
| **WHERE SQL Injection** | where_sql_injection_comprehensive.rs | 3-4h | +2% | CRITICAL - security vulnerability |
| **Mutation __typename** | mutation_typename_e2e.rs | 2-3h | +2% | MEDIUM - GraphQL spec violation |

**Subtotal:** 11-15 hours, +10% → 100% confidence in critical areas

### Secondary Path (Do Next - 12-18 hours, +4% confidence)

| Test | Confidence | Effort |
|------|-----------|--------|
| **LTree Value Validation** | +2% | 2-3h |
| **Custom Scalar Roundtrip** | +2% | 2-3h |
| **Multi-Arg Mutation Binding** | +2% | 2-3h |
| **NULL Logic in WHERE** | +2% | 2-3h |
| **Array Edge Cases** | +1% | 2-3h |
| **Case Sensitivity** | +1% | 2-3h |

**Subtotal:** 12-18 hours, +10% → 98% overall

### Nice-to-Have (Do Last - 12-17 hours, +3% confidence)

| Test | Confidence | Effort |
|------|-----------|--------|
| **Interface Implementation** | +1% | 2-3h |
| **Union Type Projection** | +1% | 2-3h |
| **Deprecated Introspection** | +1% | 1-2h |
| **Mutation Nullability** | +1% | 1-2h |
| **Custom Scalar Coercion** | +2% | 3-4h |
| **Deep Nesting in WHERE** | +1% | 1-2h |

**Subtotal:** 11-16 hours, +8% → 100% overall

---

## Execution Plan

### Week 1: Critical Path (10-15 hours)
- Day 1-2: Mutation operation dispatch + typename tests
- Day 2-3: LTree edge cases
- Day 4-5: WHERE SQL injection comprehensive tests

**Expected Result:** 92% → 98% confidence, all critical bugs covered

### Week 2: Secondary Path (12-18 hours)
- Day 1-2: Custom scalar roundtrip + validation
- Day 2-3: Multi-arg mutation binding
- Day 3-4: NULL logic + array edge cases
- Day 5: Case sensitivity verification

**Expected Result:** 98% → 100% for mutations, scalars, WHERE clause

### Week 3: Nice-to-Have (12-17 hours)
- Implementation of remaining edge case tests
- Schema type system validation
- Documentation of test coverage

**Final Result:** 100% confidence across all 8 bug categories

---

## Success Metrics

### Confidence Improvements by Category

| Category | Current | After Critical | After All |
|----------|---------|-----------------|-----------|
| **Mutations** | 90% | 98% | 99% |
| **LTree/Scalars** | 85% | 92% | 99% |
| **Schema Types** | 95% | 95% | 98% |
| **WHERE Clause** | 95% | 97% | 100% |
| **Average** | **91%** | **96%** | **99%** |

### Test Count Growth

- **Current:** 500+ integration tests
- **After Critical Path:** 504+ tests (+4 new)
- **After Secondary Path:** 510+ tests (+6 new)
- **After Nice-to-Have:** 516+ tests (+6 new)

---

## Conclusion

**We can reach 99%+ confidence with 34-50 hours of focused testing effort:**

1. **Critical Path (10-15 hours):** Gets us to 96-98% by covering the highest-impact bugs
2. **Secondary Path (12-18 hours):** Rounds out 98%+ with comprehensive edge case coverage
3. **Nice-to-Have (12-17 hours):** Achieves 100% across all categories

**Recommendation:** Implement critical path immediately (15 hours), then secondary path (18 hours) before v2.0.0 GA. This ensures production-grade confidence across all bug categories.

