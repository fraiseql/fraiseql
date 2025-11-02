# Extracted from: docs/rust/RUST_FIRST_PIPELINE.md
# Block number: 5
# Client query
query { users { id firstName } }

# Automatic extraction
field_paths = [["id"], ["firstName"]]

# Rust filters response to only include requested fields
