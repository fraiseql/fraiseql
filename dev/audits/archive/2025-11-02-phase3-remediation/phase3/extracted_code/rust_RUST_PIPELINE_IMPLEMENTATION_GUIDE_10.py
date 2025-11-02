# Extracted from: docs/rust/RUST_PIPELINE_IMPLEMENTATION_GUIDE.md
# Block number: 10
import time

# Benchmark current Rust pipeline performance
start = time.perf_counter()
for _ in range(100):
    result = await repo.find_rust("v_user", "users", info)
total_time = time.perf_counter() - start

print(f"Rust Pipeline: {total_time:.3f}s for 100 queries")
print(f"Average: {total_time / 100:.4f}s per query")
