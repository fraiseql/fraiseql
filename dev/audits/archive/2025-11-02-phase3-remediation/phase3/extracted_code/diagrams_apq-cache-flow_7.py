# Extracted from: docs/diagrams/apq-cache-flow.md
# Block number: 7
# Alert if cache hit rate drops below threshold
if cache_hit_rate < 0.8:
    alert("APQ cache hit rate below 80%")

# Alert if cache is near capacity
if cache_size > MAX_CACHE_SIZE * 0.9:
    alert("APQ cache near capacity")
