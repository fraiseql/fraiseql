# Advanced Filtering Patterns Guide

This guide demonstrates advanced filtering techniques combining SQL predicates and Rust-side filtering in fraiseql-wire.

## Filtering Architecture

fraiseql-wire supports **two-stage filtering**:

```
┌─────────────────────────────────────────────────┐
│ Postgres (Server-Side)                          │
│  SQL WHERE clause filters data at the source    │
│  ↓                                              │
│  Rows streamed to client                        │
└─────────────────────────────────────────────────┘
                      ↓
┌─────────────────────────────────────────────────┐
│ fraiseql-wire (Client-Side)                     │
│  Rust predicates refine already-filtered data   │
│  ↓                                              │
│  Final results delivered to application         │
└─────────────────────────────────────────────────┘
```

## Why Two-Stage Filtering?

### SQL Predicates (Server-Side)

**Use when**: Filtering large result sets by indexed columns

```rust
// Efficient: WHERE filters at Postgres level
client
    .query("project")
    .where_sql("project__status__name = 'active'")  // ← Postgres filters
    .execute()
    .await?
```

**Benefits**:
- Reduces network bandwidth
- Leverages Postgres indexes
- Scales to millions of rows

### Rust Predicates (Client-Side)

**Use when**: Complex logic, computed values, or cross-field validation

```rust
// Necessary: Rust logic applied to streamed rows
client
    .query("project")
    .where_rust(|json| {
        // Complex logic not expressible in SQL
        let cost = json["estimated_cost"].as_f64().unwrap_or(0.0);
        let status = json["status"]["name"].as_str().unwrap_or("");
        cost > 10_000.0 && status.starts_with("in_")
    })
    .execute()
    .await?
```

**Benefits**:
- Express arbitrary Rust logic
- No SQL injection risk
- Compose multiple predicates easily

## SQL Filtering Techniques

### 1. Simple Equality Predicates

```rust
// Single column match
client
    .query("project")
    .where_sql("project__status__name = 'active'")
    .execute()
    .await?

// Multiple conditions (AND)
client
    .query("project")
    .where_sql("project__status__name = 'active' AND project__type = 'internal'")
    .execute()
    .await?
```

### 2. Comparison Operators

```rust
// Range queries (if indexed for performance)
client
    .query("project")
    .where_sql("estimated_cost > 50000")
    .execute()
    .await?

// Multiple comparisons
client
    .query("project")
    .where_sql("estimated_cost > 50000 AND estimated_cost < 500000")
    .execute()
    .await?
```

### 3. Pattern Matching (LIKE)

```rust
// Case-sensitive prefix matching
client
    .query("project")
    .where_sql("project__name LIKE 'API%'")
    .execute()
    .await?

// Case-insensitive (ILIKE)
client
    .query("project")
    .where_sql("project__name ILIKE '%microservice%'")
    .execute()
    .await?
```

### 4. NULL Handling

```rust
// Find projects without assigned teams
client
    .query("project")
    .where_sql("team__id IS NULL")
    .execute()
    .await?

// Find projects with assigned teams
client
    .query("project")
    .where_sql("team__id IS NOT NULL")
    .execute()
    .await?
```

### 5. IN Clause for Multiple Values

```rust
// Match multiple status values
client
    .query("project")
    .where_sql("project__status__name IN ('active', 'pending', 'on_hold')")
    .execute()
    .await?
```

### 6. Boolean Logic with Parentheses

```rust
// Complex conditions with precedence
client
    .query("project")
    .where_sql(
        "(project__status__name = 'active' OR project__status__name = 'pending') \
         AND estimated_cost > 10000"
    )
    .execute()
    .await?
```

## Rust Filtering Techniques

### 1. Simple Predicates on Scalar Values

```rust
// Filter by parsed numeric field
client
    .query("project")
    .where_rust(|json| {
        let cost = json["estimated_cost"].as_f64().unwrap_or(0.0);
        cost >= 50_000.0 && cost <= 500_000.0
    })
    .execute()
    .await?
```

### 2. Nested JSON Navigation

```rust
// Access nested object fields
client
    .query("project")
    .where_rust(|json| {
        json["team"]["name"]
            .as_str()
            .map(|name| name.contains("Platform"))
            .unwrap_or(false)
    })
    .execute()
    .await?
```

### 3. Array Operations

```rust
// Filter by array contents
client
    .query("project")
    .where_rust(|json| {
        json["tags"]
            .as_array()
            .map(|tags| tags.len() > 2)
            .unwrap_or(false)
    })
    .execute()
    .await?
```

### 4. Complex Type Conversions

```rust
// Parse and validate complex types
client
    .query("project")
    .where_rust(|json| {
        match (
            json["status"]["name"].as_str(),
            json["priority"].as_i64(),
            json["created_at"].as_str(),
        ) {
            (Some("active"), Some(p), Some(date)) => {
                p >= 5 && date.starts_with("2024")
            }
            _ => false,
        }
    })
    .execute()
    .await?
```

### 5. Cross-Field Validation

```rust
// Logic across multiple fields
client
    .query("project")
    .where_rust(|json| {
        let cost = json["estimated_cost"].as_f64().unwrap_or(0.0);
        let budget = json["budget"]["allocated"].as_f64().unwrap_or(0.0);
        let status = json["status"]["name"].as_str().unwrap_or("");

        // Cost should not exceed budget for active projects
        if status == "active" {
            cost <= budget
        } else {
            true  // Inactive projects can exceed budget
        }
    })
    .execute()
    .await?
```

## Combined Filtering Strategies

### Strategy 1: Coarse SQL Filter + Fine Rust Filter

**Best for**: Large result sets requiring complex filtering

```rust
// Step 1: Filter to broad category in SQL (fast, reduces bandwidth)
// Step 2: Apply complex logic in Rust (slow, but on small result set)

client
    .query("project")
    .where_sql("project__department = 'engineering'")  // ← Reduces to ~1000 rows
    .where_rust(|json| {
        // Complex validation on remaining 1000 rows
        let stakeholders = json["stakeholders"].as_array().unwrap_or(&vec![]);
        stakeholders.iter().any(|s| {
            s["role"].as_str() == Some("lead")
        })
    })
    .execute()
    .await?
```

### Strategy 2: Composition of Multiple Rust Predicates

```rust
// Build composite predicates for reusability

fn is_high_priority(json: &serde_json::Value) -> bool {
    json["priority"].as_i64().unwrap_or(0) >= 8
}

fn is_at_risk(json: &serde_json::Value) -> bool {
    json["health_status"]["color"]
        .as_str()
        .map(|c| c == "red")
        .unwrap_or(false)
}

fn has_lead_assigned(json: &serde_json::Value) -> bool {
    json["team"]["lead_id"]
        .as_str()
        .map(|id| !id.is_empty())
        .unwrap_or(false)
}

// Combine predicates
client
    .query("project")
    .where_sql("project__status__name = 'active'")
    .where_rust(|json| {
        is_high_priority(json) &&
        (is_at_risk(json) || !has_lead_assigned(json))
    })
    .execute()
    .await?
```

### Strategy 3: Progressive Refinement

```rust
// Start broad, then progressively narrow down

let stream = client
    .query("project")
    .where_sql("created_at > NOW() - INTERVAL '1 year'")  // ← Year of projects
    .execute()
    .await?;

// Process stream with multiple passes
let high_value = stream
    .filter(|r| {
        r.as_ref()
            .ok()
            .and_then(|json| json["estimated_cost"].as_f64())
            .map(|cost| cost > 100_000.0)
            .unwrap_or(false)
    })
    .filter(|r| {
        r.as_ref()
            .ok()
            .and_then(|json| json["status"]["name"].as_str())
            .map(|status| status == "active")
            .unwrap_or(false)
    });

// Collect filtered results
let results: Vec<_> = high_value.collect::<Result<Vec<_>, _>>().await?;
```

### Strategy 4: ORDER BY + Limit Pattern

```rust
// SQL ordering reduces memory with streaming

client
    .query("project")
    .where_sql("status = 'active'")
    .order_by("estimated_cost DESC")  // ← Server sorts highest cost first
    .execute()
    .await?
    .take(10)  // Get only top 10 highest-cost projects
    .collect::<Result<Vec<_>, _>>()
    .await?
```

## Performance Optimization Patterns

### 1. Predicate Pushdown

Move as much filtering as possible to SQL:

```rust
// ❌ INEFFICIENT: Filters on client side
client
    .query("project")
    .where_rust(|json| {
        json["estimated_cost"].as_f64().unwrap_or(0.0) > 50_000.0
    })
    .execute()
    .await?

// ✅ EFFICIENT: Let Postgres filter
client
    .query("project")
    .where_sql("estimated_cost > 50000")
    .execute()
    .await?
```

### 2. Indexed Column Usage

```rust
// Assume `project__status__name` and `estimated_cost` are indexed

// ✅ GOOD: Both filters use indexed columns
client
    .query("project")
    .where_sql("project__status__name = 'active' AND estimated_cost > 50000")
    .execute()
    .await?

// ⚠️  OKAY: First filter uses index, second is slow (JSON extraction)
client
    .query("project")
    .where_sql("project__status__name = 'active'")
    .where_rust(|json| {
        json["metadata"]["custom_field"].as_str() == Some("special")
    })
    .execute()
    .await?
```

### 3. Early Termination with take()

```rust
use tokio_stream::StreamExt;

// Only collect first matching results
let first_active = client
    .query("project")
    .order_by("created_at DESC")
    .execute()
    .await?
    .filter_map(|r| async move { r.ok() })
    .take(5)  // Stop streaming after 5 matches
    .collect::<Vec<_>>()
    .await;
```

### 4. Batch Processing for Large Result Sets

```rust
// Process in chunks to avoid buffering entire result set

const CHUNK_SIZE: usize = 100;

let mut stream = client
    .query("project")
    .where_sql("status = 'active'")
    .chunk_size(CHUNK_SIZE)  // Set server-side chunk size
    .execute()
    .await?;

while let Some(result) = stream.next().await {
    match result {
        Ok(json) => {
            // Process individual row (constant memory)
            process_project(&json).await;
        }
        Err(e) => eprintln!("Stream error: {}", e),
    }
}
```

## Real-World Examples

### Example 1: Risk Dashboard Filter

Find high-priority projects at risk with unassigned leads:

```rust
client
    .query("project")
    .where_sql("status IN ('active', 'pending') AND priority > 5")
    .where_rust(|json| {
        let health = json["health_status"]["color"]
            .as_str()
            .unwrap_or("green");
        let lead_id = json["team"]["lead_id"]
            .as_str()
            .unwrap_or("");

        health == "red" && lead_id.is_empty()
    })
    .order_by("priority DESC")
    .execute()
    .await?
```

### Example 2: Budget Variance Analysis

Find projects with significant budget variance:

```rust
client
    .query("project")
    .where_sql("year = 2024")
    .where_rust(|json| {
        let allocated = json["budget"]["allocated"]
            .as_f64()
            .unwrap_or(0.0);
        let spent = json["budget"]["spent"]
            .as_f64()
            .unwrap_or(0.0);

        if allocated == 0.0 {
            return false;
        }

        let variance = (spent - allocated).abs() / allocated;
        variance > 0.2  // More than 20% variance
    })
    .order_by("variance DESC")
    .execute()
    .await?
```

### Example 3: Cross-Team Collaboration Filter

Find projects collaborating across multiple teams:

```rust
client
    .query("project")
    .where_sql("status = 'active'")
    .where_rust(|json| {
        // Count unique teams mentioned in project data
        let team_ids: std::collections::HashSet<_> = [
            json["team"]["id"].as_str(),
            json["collaborating_teams"]
                .as_array()
                .and_then(|arr| arr.first())
                .and_then(|t| t["id"].as_str()),
            json["stakeholders"]
                .as_array()
                .and_then(|arr| {
                    arr.iter()
                        .filter_map(|s| s["team_id"].as_str())
                        .next()
                })
        ]
        .iter()
        .filter_map(|id| *id)
        .collect();

        team_ids.len() >= 3  // At least 3 teams involved
    })
    .execute()
    .await?
```

## Error Handling in Filters

### Defensive Predicate Pattern

```rust
// Always handle missing/invalid fields gracefully

client
    .query("project")
    .where_rust(|json| {
        match (
            json.get("estimated_cost").and_then(|v| v.as_f64()),
            json.get("status").and_then(|v| v.get("name")).and_then(|v| v.as_str()),
        ) {
            (Some(cost), Some(status)) => cost > 50_000.0 && status == "active",
            _ => false,  // Reject malformed data
        }
    })
    .execute()
    .await?
```

### Logging Filter Behavior

```rust
use tracing::debug;

client
    .query("project")
    .where_rust(|json| {
        let passes = json["priority"]
            .as_i64()
            .map(|p| p >= 7)
            .unwrap_or(false);

        if !passes {
            debug!(
                project_id = json["id"].as_str(),
                reason = "insufficient priority"
            );
        }

        passes
    })
    .execute()
    .await?
```

## See Also

- **TYPED_STREAMING_GUIDE.md** – Type-safe result handling
- **PERFORMANCE_TUNING.md** – Query optimization strategies
- **examples/filtering.rs** – Complete working example
- **examples/advanced_filtering.rs** – More complex patterns
