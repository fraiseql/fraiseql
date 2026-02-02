# RLS Architecture Analysis: WHERE Clause Integration Points

## Current Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│ execute_regular_query() / execute_regular_query_with_security() │
│                                                                 │
│ 1. matcher.match_query()    → QueryMatch                       │
│ 2. planner.plan()           → ExecutionPlan (sql, parameters)  │
│ 3. Generate projection hint → SqlProjectionHint                │
│ 4. adapter.execute_with_projection(view, projection, HERE!, limit)
│                                             ↑                   │
│                                       WHERE_CLAUSE              │
│ 5. ResultProjector.project_results()                           │
│ 6. Serialize to JSON                                           │
└─────────────────────────────────────────────────────────────────┘
```

## Key Finding: DatabaseAdapter Already Supports WHERE Clauses

The `DatabaseAdapter` trait has two methods that handle WHERE clauses:

### 1. `execute_where_query()`
```rust
async fn execute_where_query(
    &self,
    view: &str,
    where_clause: Option<&WhereClause>,  // ← We pass RLS filter here
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<Vec<JsonbValue>>;
```

### 2. `execute_with_projection()` (PRIMARY - Used by executor)
```rust
async fn execute_with_projection(
    &self,
    view: &str,
    projection: Option<&SqlProjectionHint>,
    where_clause: Option<&WhereClause>,  // ← Currently passing None (line 597)
    limit: Option<u32>,
) -> Result<Vec<JsonbValue>>;
```

**Current state in executor.rs:597**:
```rust
let results = self
    .adapter
    .execute_with_projection(sql_source, projection_hint.as_ref(), None, None)
    .await?;
```

**This is our integration point!** The third parameter is currently `None`.

## WHERE Clause Types

From `db/where_clause.rs`:

```rust
pub enum WhereClause {
    /// Single field condition: field_name op value
    Field {
        path: Vec<String>,           // e.g., ["author_id"]
        operator: WhereOperator,     // Eq, Neq, Gt, Gte, Lt, Lte, In, Icontains, etc.
        value: serde_json::Value,    // The filter value
    },

    /// Logical AND of multiple conditions
    And(Vec<WhereClause>),

    /// Logical OR of multiple conditions
    Or(Vec<WhereClause>),

    /// Logical NOT of a condition
    Not(Box<WhereClause>),
}
```

## Established Patterns in Codebase

### Pattern 1: TenantEnforcer (runtime/tenant_enforcer.rs:58-86)

Shows exactly what we need to do:

```rust
pub fn enforce_tenant_scope(
    &self,
    where_clause: Option<&WhereClause>,
) -> Result<Option<WhereClause>, String> {
    // Build org_id filter
    let org_id_filter = WhereClause::Field {
        path: vec!["org_id".to_string()],
        operator: WhereOperator::Eq,
        value: json!(org_id),
    };

    // Compose with user's WHERE clause
    let enforced_clause = match where_clause {
        None => org_id_filter,
        Some(user_clause) => WhereClause::And(vec![
            user_clause.clone(),
            org_id_filter
        ]),
    };

    Ok(Some(enforced_clause))
}
```

**Key insight**: This is EXACTLY what RLS needs to do - compose user's WHERE with RLS filter!

### Pattern 2: AggregateParser (runtime/aggregate_parser.rs:162-184)

Shows how to build WHERE clauses from user input:

```rust
fn parse_where_clause(where_obj: &Value) -> Result<WhereClause> {
    let mut conditions = Vec::new();

    for (key, value) in obj {
        // Parse field_operator (e.g., "customer_id_eq")
        if let Some((field, operator_str)) = Self::parse_where_field_and_operator(key)? {
            let operator = WhereOperator::from_str(operator_str)?;

            conditions.push(WhereClause::Field {
                path: vec![field.to_string()],
                operator,
                value: value.clone(),
            });
        }
    }

    Ok(WhereClause::And(conditions))
}
```

## RLS Integration Strategy

### Integration Point: Cycle 3 Implementation

**Where**: `execute_regular_query_with_security()` (executor.rs, after line 550)

**What to do**:
```rust
async fn execute_regular_query_with_security(
    &self,
    query: &str,
    variables: Option<&serde_json::Value>,
    security_context: &SecurityContext,
) -> Result<String> {
    // 1. Validate security context
    if security_context.is_expired() {
        return Err(FraiseQLError::Validation { ... });
    }

    // 2. Match query to compiled template
    let query_match = self.matcher.match_query(query, variables)?;

    // 3. Create execution plan
    let plan = self.planner.plan(&query_match)?;

    // 4. ← NEW: EVALUATE RLS POLICY AND BUILD WHERE CLAUSE
    let rls_where_clause: Option<WhereClause> = if let Some(ref policy) = self.config.rls_policy {
        policy.evaluate(security_context, &query_match.query_def.name)?
    } else {
        None
    };

    // 5. Generate projection hint (unchanged)
    let projection_hint = ... ;

    // 6. ← MODIFIED: Pass RLS where_clause to execute_with_projection
    let results = self
        .adapter
        .execute_with_projection(sql_source, projection_hint.as_ref(), rls_where_clause.as_ref(), None)
        .await?;

    // 7. Project and return (unchanged)
    ...
}
```

### Benefits of This Approach

1. **Type-safe**: WhereClause composition prevents SQL injection
2. **Database-agnostic**: DatabaseAdapter handles SQL generation per database
3. **Leverages existing pattern**: TenantEnforcer already does this
4. **Minimal changes**: Only 2 lines differ from current code
5. **Performance**: Filtering happens at DB level, not in Rust memory
6. **Composable**: WhereClause::And() naturally combines multiple filters

### No Changes Needed to

- **ExecutionPlan**: Already has sql field (no where_clause field needed)
- **QueryPlanner**: No need to modify - WHERE clause application is post-planning
- **DatabaseAdapter interface**: Already supports where_clause parameter
- **SQL generation**: Handled by individual DatabaseAdapter implementations

## File Locations Reference

| Component | File | Lines | Purpose |
|-----------|------|-------|---------|
| WhereClause enum | `db/where_clause.rs` | 39-58 | Type definition |
| WhereOperator enum | `db/where_clause.rs` | 76-130 | Comparison operators |
| DatabaseAdapter trait | `db/traits.rs` | 187-193 | execute_with_projection signature |
| TenantEnforcer pattern | `runtime/tenant_enforcer.rs` | 58-86 | Composition pattern |
| Current executor call | `runtime/executor.rs` | 595-598 | WHERE clause = None |
| execute_regular_query_with_security | `runtime/executor.rs` | 538-558 | Target for modification |
| SecurityContext struct | `security/security_context.rs` | 27-100 | User info |
| RLSPolicy trait | `security/rls_policy.rs` | 44-71 | Policy evaluation |

## Implementation Checklist for Cycle 3

- [ ] **Import**: Add `use crate::db::WhereClause;` to executor.rs (if not already imported)
- [ ] **Build RLS filter**: Call `rls_policy.evaluate()` to get `Option<WhereClause>`
- [ ] **Pass to adapter**: Change `execute_with_projection(..., None, None)` to `execute_with_projection(..., rls_where_clause.as_ref(), None)`
- [ ] **Test**: Verify non-admin user only sees filtered rows
- [ ] **Document**: Add code comments explaining RLS filtering

## Example Test Case

```rust
#[tokio::test]
async fn test_rls_filters_regular_query_results() {
    // Setup
    let schema = create_test_schema();
    let adapter = Arc::new(MockAdapter::with_rls_results());

    let config = RuntimeConfig::default()
        .with_rls_policy(Arc::new(DefaultRLSPolicy::new()));

    let executor = Executor::with_config(schema, adapter, config);

    // User context
    let user_context = SecurityContext {
        user_id: "user123".to_string(),
        roles: vec!["user".to_string()],
        tenant_id: None,
        scopes: vec![],
        attributes: HashMap::new(),
        request_id: "req-1".to_string(),
        ip_address: None,
        authenticated_at: Utc::now(),
        expires_at: Utc::now() + Duration::hours(1),
        issuer: None,
        audience: None,
    };

    // Execute with RLS
    let query = r#"query { posts { id title } }"#;
    let result = executor.execute_with_security(query, None, &user_context).await.unwrap();

    // Verify: Only posts with author_id = user123 returned
    let json: serde_json::Value = serde_json::from_str(&result).unwrap();
    let posts = json["data"]["posts"].as_array().unwrap();

    // All posts should have author_id == user123
    for post in posts {
        assert_eq!(post["author_id"], "user123");
    }
}
```

## Summary

The architecture is **already designed for WHERE clause filtering**. We just need to:

1. Evaluate RLS policy → `Option<WhereClause>`
2. Pass to `execute_with_projection()` where it's already supported
3. DatabaseAdapter implementations handle the rest

This follows the exact pattern used by TenantEnforcer, ensuring consistency with the codebase.
