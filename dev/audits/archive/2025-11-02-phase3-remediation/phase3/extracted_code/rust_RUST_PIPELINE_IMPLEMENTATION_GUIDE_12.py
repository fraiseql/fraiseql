# Extracted from: docs/rust/RUST_PIPELINE_IMPLEMENTATION_GUIDE.md
# Block number: 12
# Update return types
async def users(info) -> RustResponseBytes:  # Correct
async def users(info) -> list[User]:         # Wrong for Rust pipeline
