# Extracted from: docs/rust/RUST_PIPELINE_IMPLEMENTATION_GUIDE.md
# Block number: 4
# Client queries only specific fields
query {
  users {
    id
    firstName  # Only these fields processed
  }
}

# Rust automatically filters JSONB response
# No Python overhead for unused fields
