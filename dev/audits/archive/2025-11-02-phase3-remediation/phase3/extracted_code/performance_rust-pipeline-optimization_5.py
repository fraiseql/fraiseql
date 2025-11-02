# Extracted from: docs/performance/rust-pipeline-optimization.md
# Block number: 5
import time

start = time.time()
result = await repo.find("v_user")
duration = time.time() - start
print(f"Total time: {duration * 1000:.2f}ms")
