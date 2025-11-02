# Extracted from: docs/rust/RUST_FIRST_PIPELINE.md
# Block number: 4
from fraiseql import type


# Schema definition
@type
class User:
    first_name: str
    last_name: str


# Automatic registration happens during startup
# Rust knows how to transform User types
