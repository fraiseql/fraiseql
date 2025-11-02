# Extracted from: docs/performance/rust-pipeline-optimization.md
# Block number: 4
from fraiseql.monitoring import get_metrics

metrics = get_metrics()
print(f"Rust transform avg: {metrics['rust_transform_avg_ms']}ms")
print(f"Rust transform p95: {metrics['rust_transform_p95_ms']}ms")
