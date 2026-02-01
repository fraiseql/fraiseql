# Getting Started with FraiseQL v2

**Duration**: ~15 minutes
**Outcome**: Running your first GraphQL query
**Prerequisites**: Rust 1.70+, basic terminal knowledge

---

## Step 1: Installation (2 minutes)

### Add to your project

```bash
cargo new my-graphql-api
cd my-graphql-api
```

### Add FraiseQL to `Cargo.toml`

```toml
[dependencies]
fraiseql = "2.0"
tokio = { version = "1", features = ["full"] }
serde_json = "1.0"
```

### Verify installation

```bash
cargo check
```

Expected output: `Finished 'dev' profile`

✅ **Installation complete!**

---

## Step 2: Create Your Schema (3 minutes)

### Create `schema.json`

Save this as `schema.json` in your project root:

```json
{
  "types": [
    {
      "name": "User",
      "fields": [
        {
          "name": "id",
          "type": "ID",
          "nonNull": true
        },
        {
          "name": "name",
          "type": "String",
          "nonNull": true
        },
        {
          "name": "email",
          "type": "String",
          "nonNull": true
        }
      ]
    }
  ],
  "queries": [
    {
      "name": "users",
      "returnType": "User",
      "isList": true,
      "args": []
    },
    {
      "name": "user",
      "returnType": "User",
      "isList": false,
      "args": [
        {
          "name": "id",
          "type": "ID",
          "nonNull": true
        }
      ]
    }
  ]
}
```

### What this means:

- **User type**: Represents a user with `id`, `name`, `email` fields
- **users query**: Returns a list of all users
- **user query**: Returns a single user by ID

✅ **Schema created!**

---

## Step 3: Hello World (5 minutes)

### Write your first program

Replace `src/main.rs` with:

```rust
use fraiseql::schema::CompiledSchema;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load your schema
    let schema = CompiledSchema::from_file("schema.json")?;

    // Write a simple query
    let query = r#"
        query {
            users {
                id
                name
                email
            }
        }
    "#;

    // Execute the query
    let result = schema.execute(query).await?;

    // Print the result
    println!("Query result:");
    println!("{}", serde_json::to_string_pretty(&result)?);

    Ok(())
}
```

### Run it

```bash
cargo run
```

Expected output:
```
Query result:
{
  "data": {
    "users": [
      {
        "id": "1",
        "name": "Alice",
        "email": "alice@example.com"
      },
      {
        "id": "2",
        "name": "Bob",
        "email": "bob@example.com"
      }
    ]
  }
}
```

**If you get an error:**
- `schema.json not found`: Make sure `schema.json` is in your project root
- `compile error`: Run `cargo check` to see the exact error

✅ **First query executed!**

---

## Step 4: Next Steps (5 minutes)

### Learn more

- **Common Patterns**: See [PATTERNS.md](guides/PATTERNS.md) for solutions to real-world problems
- **Deployment**: When ready to deploy, see [deployment guide](deployment/guide.md)
- **Operations**: Manage your FraiseQL instance with [operations guide](operations/guide.md)

### Try these exercises

1. **Add a field**: Add a `age` field to the User type
2. **Create a mutation**: Add a `createUser` mutation
3. **Query a single user**: Modify the query to fetch one user by ID
4. **Error handling**: Catch and handle errors in your code

### Common questions

**Q: Where do I put my database code?**
A: Replace the mock data in the schema execution with real database queries. See [PATTERNS.md](guides/PATTERNS.md) for examples.

**Q: How do I add authentication?**
A: See the authentication pattern in [PATTERNS.md](guides/PATTERNS.md).

**Q: Can I use this in production?**
A: Yes! See [deployment guide](deployment/guide.md) and [operations guide](operations/guide.md).

---

## Troubleshooting

### "cannot find schema file"
- Make sure `schema.json` is in your project root (same level as `Cargo.toml`)
- Use absolute path: `CompiledSchema::from_file("/path/to/schema.json")?`

### "serde_json not found"
- Add to `Cargo.toml`: `serde_json = "1.0"`
- Run: `cargo build`

### "tokio not found"
- Add to `Cargo.toml`: `tokio = { version = "1", features = ["full"] }`
- Run: `cargo build`

### "Query failed with error"
- Check your query syntax against the schema
- Use the error message to identify the issue
- See [patterns guide](guides/PATTERNS.md) for query help

---

## What You've Learned

✅ Installed FraiseQL
✅ Created a GraphQL schema
✅ Executed a query
✅ Handled results and errors

**Next**: Continue with [PATTERNS.md](guides/PATTERNS.md) for deeper understanding.

---

**Questions?** See [TROUBLESHOOTING.md](TROUBLESHOOTING.md) for FAQ and solutions, or open an issue on [GitHub](https://github.com/fraiseql/fraiseql-v2).
