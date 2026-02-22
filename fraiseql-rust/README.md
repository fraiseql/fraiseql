# fraiseql-rust

> **Status: Not yet implemented.**

The Rust authoring SDK for FraiseQL is planned but not yet built.

## What it will provide

A proc-macro-based way to define FraiseQL schemas in Rust that compile to `schema.json`:

```rust
// Planned API (subject to change)
use fraiseql::prelude::*;

#[fraiseql::type]
struct User {
    id: i32,
    name: String,
    email: Option<String>,
}

#[fraiseql::query(sql_source = "v_users")]
fn users(limit: Option<i32>) -> Vec<User> {}

fn main() {
    fraiseql::export_schema("schema.json").unwrap();
}
```

## Note on complexity

A full Rust authoring SDK requires procedural macros for type introspection at
compile time. This is non-trivial to implement correctly. If you need schema
authoring today, the Python or TypeScript SDKs are the easiest to use alongside
a Rust runtime.

## Alternatives

The following SDKs are production-ready today:

- [fraiseql-python](../fraiseql-python) — reference implementation
- [fraiseql-typescript](../fraiseql-typescript)
- [fraiseql-java](../fraiseql-java)
- [fraiseql-php](../fraiseql-php)
- [fraiseql-go](../fraiseql-go)

## Contributing

Contributions welcome. See the Python SDK for the reference authoring API and
the expected `schema.json` output format. Proc macro implementation should live
in a `fraiseql-rust-macros` sub-crate.
