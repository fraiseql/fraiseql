# Extracted from: docs/rust/RUST_PIPELINE_IMPLEMENTATION_GUIDE.md
# Block number: 11
# Always use Rust pipeline methods for best performance
result = await repo.find_rust("table", "field", info)  # Optimal
