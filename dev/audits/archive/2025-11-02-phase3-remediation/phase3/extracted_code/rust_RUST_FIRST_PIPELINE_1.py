# Extracted from: docs/rust/RUST_FIRST_PIPELINE.md
# Block number: 1
# New Rust pipeline methods (recommended)
result = await repo.find_rust("v_user", "users", info)
single = await repo.find_one_rust("v_user", "user", info, id=user_id)

# Legacy methods still available
result = await repo.find("v_user")  # Slower Python path
