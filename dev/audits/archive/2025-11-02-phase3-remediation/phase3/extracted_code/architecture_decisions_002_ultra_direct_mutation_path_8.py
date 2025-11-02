# Extracted from: docs/architecture/decisions/002_ultra_direct_mutation_path.md
# Block number: 8
import time


async def benchmark_mutation_paths():
    """Compare standard vs. ultra-direct mutation performance."""
    # Warmup
    for _ in range(10):
        await delete_customer_standard("uuid-test")
        await delete_customer_ultra_direct("uuid-test")

    # Benchmark standard path
    start = time.perf_counter()
    for _ in range(1000):
        await delete_customer_standard("uuid-test")
    standard_time = time.perf_counter() - start

    # Benchmark ultra-direct path
    start = time.perf_counter()
    for _ in range(1000):
        await delete_customer_ultra_direct("uuid-test")
    direct_time = time.perf_counter() - start

    speedup = standard_time / direct_time
    print(f"Standard: {standard_time:.3f}s")
    print(f"Direct:   {direct_time:.3f}s")
    print(f"Speedup:  {speedup:.1f}x faster")

    assert speedup > 2.0, "Ultra-direct path should be >2x faster"
