# Extracted from: docs/rust/RUST_PIPELINE_IMPLEMENTATION_GUIDE.md
# Block number: 13
# Ensure GraphQL info is passed
return await repo.find_rust("v_user", "users", info)  # info required
# Not: return await repo.find_rust("v_user", "users") # Missing info
