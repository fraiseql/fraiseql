# Phase 1 Implementation: Parameterize LIMIT/OFFSET

**Objective**: Convert LIMIT/OFFSET from string interpolation to parameterized queries across all database adapters

**Estimated Effort**: 5-7 hours
**Priority**: P1 (Best Practice, Non-Critical)
**Risk Level**: Very Low

---

## Task 1: PostgreSQL Adapter

**File**: `crates/fraiseql-core/src/db/postgres/adapter.rs` (lines 287-294)

### Current Code

```rust
// Add LIMIT
if let Some(lim) = limit {
    sql.push_str(&format!(" LIMIT {lim}"));
}

// Add OFFSET
if let Some(off) = offset {
    sql.push_str(&format!(" OFFSET {off}"));
}
```

### New Implementation

```rust
// PostgreSQL supports parameterized LIMIT/OFFSET
// Add parameters to the values vector and generate placeholders
let mut next_param = params.len() + 1;

// Add LIMIT as parameterized query
if let Some(lim) = limit {
    sql.push_str(&format!(" LIMIT ${next_param}"));
    params.push(Value::I32(lim as i32));
    next_param += 1;
}

// Add OFFSET as parameterized query
if let Some(off) = offset {
    sql.push_str(&format!(" OFFSET ${next_param}"));
    params.push(Value::I32(off as i32));
    next_param += 1;
}
```

### Tests to Add

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_limit_parameterization() {
        let mut sql = String::from("SELECT * FROM users");
        let mut params = vec![];

        let limit = Some(10u32);
        let offset = Some(20u32);

        // Simulate the new code
        let mut next_param = params.len() + 1;
        if let Some(lim) = limit {
            sql.push_str(&format!(" LIMIT ${next_param}"));
            params.push(Value::I32(lim as i32));
            next_param += 1;
        }
        if let Some(off) = offset {
            sql.push_str(&format!(" OFFSET ${next_param}"));
            params.push(Value::I32(off as i32));
        }

        // Verify SQL structure
        assert!(sql.contains("LIMIT $1"));
        assert!(sql.contains("OFFSET $2"));

        // Verify parameters
        assert_eq!(params.len(), 2);
        match (&params[0], &params[1]) {
            (Value::I32(10), Value::I32(20)) => (),
            _ => panic!("Parameters not bound correctly"),
        }
    }

    #[test]
    fn test_limit_only() {
        let mut sql = String::from("SELECT * FROM users");
        let mut params = vec![];

        let limit = Some(10u32);
        let offset = None;

        let mut next_param = params.len() + 1;
        if let Some(lim) = limit {
            sql.push_str(&format!(" LIMIT ${next_param}"));
            params.push(Value::I32(lim as i32));
            next_param += 1;
        }
        if let Some(off) = offset {
            sql.push_str(&format!(" OFFSET ${next_param}"));
            params.push(Value::I32(off as i32));
        }

        assert!(sql.contains("LIMIT $1"));
        assert!(!sql.contains("OFFSET"));
        assert_eq!(params.len(), 1);
    }

    #[test]
    fn test_no_limit_offset() {
        let mut sql = String::from("SELECT * FROM users");
        let params: Vec<Value> = vec![];

        let limit = None;
        let offset = None;

        let mut next_param = params.len() + 1;
        if let Some(lim) = limit {
            sql.push_str(&format!(" LIMIT ${next_param}"));
        }
        if let Some(off) = offset {
            sql.push_str(&format!(" OFFSET ${next_param}"));
        }

        assert!(!sql.contains("LIMIT"));
        assert!(!sql.contains("OFFSET"));
    }
}
```

### Verification Steps

1. Ensure PostgreSQL driver supports parameterized LIMIT
2. Run: `cargo test --lib db::postgres::adapter`
3. Check that query plan caching works (should use $1, $2 parameters)
4. Verify with actual database:

   ```sql
   PREPARE test (INT, INT) AS SELECT * FROM users LIMIT $1 OFFSET $2;
   EXECUTE test(10, 20);
   ```

---

## Task 2: MySQL Adapter

**File**: `crates/fraiseql-core/src/db/mysql/adapter.rs` (lines 196-203)

### Current Code

```rust
// Add LIMIT
if let Some(lim) = limit {
    sql.push_str(&format!(" LIMIT {lim}"));
}

// Add OFFSET
if let Some(off) = offset {
    sql.push_str(&format!(" OFFSET {off}"));
}
```

### New Implementation

```rust
// MySQL supports ? placeholders
// Add parameters to the values vector
if let Some(lim) = limit {
    sql.push_str(" LIMIT ?");
    params.push(Value::I32(lim as i32));
}

if let Some(off) = offset {
    sql.push_str(" OFFSET ?");
    params.push(Value::I32(off as i32));
}
```

### Tests to Add

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mysql_limit_offset_parameters() {
        let mut sql = String::from("SELECT * FROM users");
        let mut params = vec![];

        let limit = Some(10u32);
        let offset = Some(20u32);

        if let Some(lim) = limit {
            sql.push_str(" LIMIT ?");
            params.push(Value::I32(lim as i32));
        }
        if let Some(off) = offset {
            sql.push_str(" OFFSET ?");
            params.push(Value::I32(off as i32));
        }

        // Verify SQL uses ? placeholders
        assert!(sql.contains("LIMIT ?"));
        assert!(sql.contains("OFFSET ?"));

        // Verify parameter count matches placeholders
        assert_eq!(params.len(), 2);
        assert_eq!(sql.matches('?').count(), 2);
    }
}
```

### Verification Steps

1. Confirm MySQL accepts `LIMIT ? OFFSET ?`
2. Run: `cargo test --lib db::mysql::adapter`
3. Test with actual MySQL database

---

## Task 3: SQLite Adapter

**File**: `crates/fraiseql-core/src/db/sqlite/adapter.rs` (lines 211-218)

### Current Code

```rust
// Add LIMIT
if let Some(lim) = limit {
    sql.push_str(&format!(" LIMIT {lim}"));
}

// Add OFFSET
if let Some(off) = offset {
    sql.push_str(&format!(" OFFSET {off}"));
}
```

### New Implementation

```rust
// SQLite supports ? placeholders
if let Some(lim) = limit {
    sql.push_str(" LIMIT ?");
    params.push(Value::I32(lim as i32));
}

if let Some(off) = offset {
    sql.push_str(" OFFSET ?");
    params.push(Value::I32(off as i32));
}
```

### Tests to Add

Similar to MySQL (see Task 2)

### Verification Steps

1. Run: `cargo test --lib db::sqlite::adapter`
2. Test with actual SQLite database

---

## Task 4: SQL Server Adapter

**File**: `crates/fraiseql-core/src/db/sqlserver/adapter.rs` (similar lines)

### Current Code

```rust
// Find and replace similar LIMIT/OFFSET handling
```

### New Implementation

SQL Server uses different syntax (OFFSET/FETCH), but should follow same pattern:

```rust
// SQL Server uses OFFSET ... ROWS FETCH ... ROWS ONLY
if let Some(off) = offset {
    sql.push_str(" OFFSET ? ROWS");
    params.push(Value::I32(off as i32));
}

if let Some(lim) = limit {
    sql.push_str(" FETCH NEXT ? ROWS ONLY");
    params.push(Value::I32(lim as i32));
}
```

### Tests to Add

Similar pattern to other adapters

---

## Task 5: Integration Testing

**Location**: `tests/integration/` directory

### Create New Test File: `tests/integration/limit_offset_test.rs`

```rust
#[cfg(test)]
mod limit_offset_tests {
    use fraiseql_core::db::*;
    use sqlx::pool::Pool;

    #[tokio::test]
    async fn test_postgres_limit_offset_e2e() {
        let pool = create_postgres_test_pool().await;

        // Insert test data
        for i in 1..=100 {
            sqlx::query("INSERT INTO test_data (id, name) VALUES ($1, $2)")
                .bind(i)
                .bind(format!("Item {}", i))
                .execute(&pool)
                .await
                .unwrap();
        }

        // Test LIMIT only
        let result = sqlx::query("SELECT * FROM test_data LIMIT $1")
            .bind(10i32)
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(result.len(), 10);

        // Test LIMIT and OFFSET
        let result = sqlx::query("SELECT * FROM test_data LIMIT $1 OFFSET $2")
            .bind(10i32)
            .bind(20i32)
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(result.len(), 10);

        // Verify rows 21-30 are returned
        let first_id: i32 = result[0].get(0);
        assert_eq!(first_id, 21);
    }

    #[tokio::test]
    async fn test_mysql_limit_offset_e2e() {
        let pool = create_mysql_test_pool().await;

        // Insert test data
        for i in 1..=100 {
            sqlx::query("INSERT INTO test_data (id, name) VALUES (?, ?)")
                .bind(i)
                .bind(format!("Item {}", i))
                .execute(&pool)
                .await
                .unwrap();
        }

        // Test parameterized LIMIT/OFFSET
        let result = sqlx::query("SELECT * FROM test_data LIMIT ? OFFSET ?")
            .bind(10i32)
            .bind(20i32)
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(result.len(), 10);
    }

    #[tokio::test]
    async fn test_sqlite_limit_offset_e2e() {
        let pool = create_sqlite_test_pool().await;

        // Similar test structure
        let result = sqlx::query("SELECT * FROM test_data LIMIT ? OFFSET ?")
            .bind(10i32)
            .bind(20i32)
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(result.len(), 10);
    }

    #[tokio::test]
    async fn test_sqlserver_limit_offset_e2e() {
        let pool = create_sqlserver_test_pool().await;

        // SQL Server uses OFFSET ... ROWS FETCH ... ROWS ONLY
        let result = sqlx::query("SELECT * FROM test_data OFFSET ? ROWS FETCH NEXT ? ROWS ONLY")
            .bind(20i32)
            .bind(10i32)
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(result.len(), 10);
    }
}
```

### Cross-Database Compatibility Tests

```rust
#[tokio::test]
async fn test_limit_offset_consistency_across_databases() {
    // Setup: Create identical test datasets in all databases
    let pg_pool = create_postgres_test_pool().await;
    let mysql_pool = create_mysql_test_pool().await;
    let sqlite_pool = create_sqlite_test_pool().await;
    let sqlserver_pool = create_sqlserver_test_pool().await;

    // Insert 50 rows in each database
    for (pool, is_pg) in [(&pg_pool, true), (&mysql_pool, false), (&sqlite_pool, false), (&sqlserver_pool, false)] {
        for i in 1..=50 {
            let query = if is_pg {
                "INSERT INTO test_data (id, value) VALUES ($1, $2)"
            } else {
                "INSERT INTO test_data (id, value) VALUES (?, ?)"
            };
            // Execute insert...
        }
    }

    // Query with LIMIT 10, OFFSET 20 from all databases
    let results = vec![
        fetch_with_limit_offset(&pg_pool, 10, 20, true).await,
        fetch_with_limit_offset(&mysql_pool, 10, 20, false).await,
        fetch_with_limit_offset(&sqlite_pool, 10, 20, false).await,
        fetch_with_limit_offset(&sqlserver_pool, 10, 20, false).await,
    ];

    // Verify all databases return rows 21-30 (10 rows starting at offset 20)
    for result_set in results {
        assert_eq!(result_set.len(), 10);
        assert_eq!(result_set[0].id, 21);
        assert_eq!(result_set[9].id, 30);
    }
}
```

---

## Verification Checklist

### Before Implementation

- [ ] Review current LIMIT/OFFSET implementations in all adapters
- [ ] Verify each database supports parameterized LIMIT/OFFSET syntax
- [ ] Check Value enum supports I32 type for binding

### During Implementation

- [ ] PostgreSQL adapter updated and tested locally
- [ ] MySQL adapter updated and tested locally
- [ ] SQLite adapter updated and tested locally
- [ ] SQL Server adapter updated and tested locally
- [ ] All unit tests pass: `cargo test --all`
- [ ] Clippy checks pass: `cargo clippy --all-targets --all-features`

### After Implementation

- [ ] Integration tests pass against real databases
- [ ] Performance testing shows no regression (query plans should be similar)
- [ ] Code review completed
- [ ] Committed with descriptive commit message
- [ ] Full regression test suite passes

---

## Rollback Plan

If issues arise during implementation:

1. **Unit tests fail**: Verify syntax with database documentation, adjust placeholder format
2. **Integration tests fail**: Check parameter binding order, ensure params vector matches placeholders
3. **Performance regression**: Compare query plans before/after, verify indexes still used
4. **Critical issue**: Revert commits with `git revert` and restart analysis

---

## Estimated Breakdown

| Task | Subtask | Hours | Notes |
|------|---------|-------|-------|
| 1 | PostgreSQL adapter | 1.5 | Most complex (numbered placeholders) |
| 2 | MySQL adapter | 1 | Simpler (? placeholders) |
| 3 | SQLite adapter | 1 | Same pattern as MySQL |
| 4 | SQL Server adapter | 0.5 | Different syntax but straightforward |
| 5 | Integration testing | 1.5 | Cross-database compatibility |
| **Total** | | **5.5 hours** | |

---

## Success Criteria

✅ All LIMIT/OFFSET values are passed as parameters, never interpolated
✅ Query syntax remains identical to current behavior
✅ All tests pass across all supported databases
✅ Performance metrics show no regression or improvement
✅ Code follows project style guidelines
✅ Documentation updated to reflect parameterization

---

## Next Steps

1. Assign implementation tasks to team members
2. Create GitHub issues for tracking
3. Start with PostgreSQL adapter (most complex)
4. Use this spec for code review
5. Merge when all tests pass and reviews complete
