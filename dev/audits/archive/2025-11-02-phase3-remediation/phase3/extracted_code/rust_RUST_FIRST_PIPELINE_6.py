# Extracted from: docs/rust/RUST_FIRST_PIPELINE.md
# Block number: 6
# Old code
return await repo.find("users")

# New code (recommended)
return await repo.find_rust("users", "users", info)
