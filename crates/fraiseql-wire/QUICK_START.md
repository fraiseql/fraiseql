# Quick Start Guide

Get started with fraiseql-wire in 5 minutes.

---

## Installation

### Add to Cargo.toml

```toml
[dependencies]
fraiseql-wire = "0.1"
tokio = { version = "1", features = ["full"] }
futures = "0.3"
serde_json = "1"
```

### Verify Installation

```bash
cargo check
```

---

## Running Postgres Locally

### Option 1: Docker (Recommended)

```bash
cd fraiseql-wire
docker-compose up -d
```

This starts Postgres 17 with test data and creates the `fraiseql_test` database.

```bash
# Check status
docker-compose ps

# View logs
docker-compose logs -f postgres

# Stop
docker-compose down
```

### Option 2: Native Installation

Install Postgres 17:

```bash
# macOS
brew install postgresql@17

# Linux (Ubuntu)
sudo apt-get install postgresql-17

# Start service
pg_ctl -D /usr/local/var/postgres start
```

Initialize test database:

```bash
createdb -U postgres fraiseql_test
psql -U postgres -d fraiseql_test -f tests/fixtures/schema.sql
psql -U postgres -d fraiseql_test -f tests/fixtures/seed_data.sql
```

### Option 3: Cloud Postgres

Set environment variables for your cloud Postgres:

```bash
export POSTGRES_HOST=your-host.compute-1.amazonaws.com
export POSTGRES_USER=postgres
export POSTGRES_PASSWORD=your-password
export POSTGRES_DB=fraiseql_test
```

---

## First Program

Create `src/main.rs`:

```rust
use fraiseql_wire::client::FraiseClient;
use futures::stream::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect to Postgres
    let client = FraiseClient::connect("postgres://localhost/fraiseql_test").await?;
    println!("✓ Connected to Postgres\n");

    // Stream all rows
    let mut stream = client.query("projects").execute().await?;

    println!("Streaming projects:\n");
    let mut count = 0;
    while let Some(item) = stream.next().await {
        let json = item?;
        println!("  {}: {}", count + 1, json);
        count += 1;
        if count >= 5 {
            println!("  ... (showing first 5)");
            break;
        }
    }

    Ok(())
}
```

Run it:

```bash
cargo run
```

Output:

```
✓ Connected to Postgres

Streaming projects:

  1: {"id":"123e4567-e89b-12d3-a456-426614174000","name":"Project A","status":"active"}
  2: {"id":"223e4567-e89b-12d3-a456-426614174001","name":"Project B","status":"active"}
  ...
```

---

## Next Steps

### Filtering

```rust
// Filter on the server (efficient)
let stream = client
    .query("projects")
    .where_sql("data->>'status' = 'active'")
    .execute()
    .await?;

// Filter on the client (flexible)
let stream = client
    .query("projects")
    .where_rust(|json| {
        json["estimated_cost"].as_f64().unwrap_or(0.0) > 10000.0
    })
    .execute()
    .await?;

// Combine both
let stream = client
    .query("projects")
    .where_sql("data->>'status' = 'active'")
    .where_rust(|json| json["priority"].as_str() == Some("high"))
    .execute()
    .await?;
```

### Sorting

```rust
// Server-side sorting (efficient)
let stream = client
    .query("projects")
    .order_by("data->>'name' ASC")
    .execute()
    .await?;

// With collation
let stream = client
    .query("projects")
    .order_by("data->>'name' COLLATE \"C\" DESC")
    .execute()
    .await?;
```

### Streaming Large Result Sets

```rust
// Control chunk size (default is 256 rows)
let stream = client
    .query("projects")
    .chunk_size(512)  // Fewer round-trips, slightly more memory
    .execute()
    .await?;

// Or smaller for bounded memory
let stream = client
    .query("projects")
    .chunk_size(64)   // More round-trips, less memory
    .execute()
    .await?;
```

### Error Handling

```rust
use futures::stream::StreamExt;

let mut stream = client.query("projects").execute().await?;

while let Some(item) = stream.next().await {
    match item {
        Ok(json) => println!("{}", json),
        Err(e) => {
            eprintln!("Error processing row: {}", e);
            // For detailed diagnosis, see TROUBLESHOOTING.md
            return Err(Box::new(e));
        }
    }
}
```

---

## Running Examples

See the `examples/` directory:

```bash
# Basic streaming
cargo run --example basic_query

# Filtering (SQL and Rust predicates)
cargo run --example filtering

# Ordering (ORDER BY)
cargo run --example ordering

# Large result handling
cargo run --example streaming

# Error handling patterns
cargo run --example error_handling
```

---

## Running Tests

```bash
# Unit tests (no database required)
cargo test --lib

# Integration tests (requires Postgres running)
cargo test --test integration -- --ignored --nocapture

# Streaming performance tests
cargo test --test streaming_integration -- --ignored --nocapture

# Load tests
cargo test --test load_tests -- --ignored --nocapture

# All tests
cargo test -- --ignored --nocapture
```

---

## Common Patterns

### Collecting Results

```rust
// Collect all results into a Vec
let results: Vec<serde_json::Value> =
    stream.collect::<Result<_, _>>()?;
```

### Processing in Batches

```rust
use futures::stream::StreamExt;

let mut stream = client.query("projects").execute().await?;
let mut batch = Vec::new();

while let Some(item) = stream.next().await {
    batch.push(item?);
    if batch.len() >= 100 {
        process_batch(&batch).await?;
        batch.clear();
    }
}

if !batch.is_empty() {
    process_batch(&batch).await?;
}
```

### Mapping Values

```rust
let stream = client.query("projects").execute().await?;

let names: Vec<String> = stream
    .filter_map(|result| async move {
        result.ok().and_then(|json| {
            json.get("name").and_then(|v| v.as_str()).map(|s| s.to_string())
        })
    })
    .collect()
    .await;
```

---

## Troubleshooting

### Connection Refused

```
Error: Connection failed: connection refused
```

Check that Postgres is running:

```bash
# Docker
docker-compose ps

# Native
psql -h localhost -U postgres
```

### Authentication Failed

```
Error: Authentication failed for user 'postgres'
```

Check credentials:

```bash
psql -h localhost -U postgres -W
```

### Invalid Result Schema

```
Error: Query returned 2 columns instead of 1
```

Ensure query returns only `SELECT data` from a view:

```sql
-- ✓ Correct
SELECT data FROM v_projects

-- ✗ Wrong
SELECT data, id FROM v_projects
```

For more detailed diagnosis, see [TROUBLESHOOTING.md](TROUBLESHOOTING.md).

---

## Documentation

* **[README.md](README.md)** – Project overview and features
* **[TESTING_GUIDE.md](TESTING_GUIDE.md)** – How to run tests
* **[TROUBLESHOOTING.md](TROUBLESHOOTING.md)** – Error diagnosis
* **[CI_CD_GUIDE.md](CI_CD_GUIDE.md)** – Development and release workflows
* **[CONTRIBUTING.md](CONTRIBUTING.md)** – Contributing guidelines
* **[PRD.md](PRD.md)** – Product requirements and design

---

## API Reference

Full API documentation:

```bash
cargo doc --no-deps --open
```

Key types:

* `FraiseClient` – Main entry point for queries
* `QueryBuilder` – Fluent query construction
* `Stream<Item = Result<Value>>` – Result stream
* `FraiseError` – Error type with diagnostic messages

---

## Getting Help

* **Documentation**: See guides above
* **Examples**: Check `examples/` directory
* **Issues**: [GitHub Issues](https://github.com/fraiseql/fraiseql-wire/issues)
* **Discussions**: [GitHub Discussions](https://github.com/fraiseql/fraiseql-wire/discussions)

---

**You're ready to stream JSON from Postgres!** 🚀
