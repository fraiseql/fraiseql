# Extracted from: docs/rust/RUST_PIPELINE_IMPLEMENTATION_GUIDE.md
# Block number: 9
# All queries use the exclusive Rust pipeline
result = await repo.find_rust("v_user", "users", info)

# Performance benefits:
# - Pre-allocated buffers, no Python GC pressure
# - Direct UTF-8 encoding for HTTP responses
# - 7-10x faster than traditional JSON processing
