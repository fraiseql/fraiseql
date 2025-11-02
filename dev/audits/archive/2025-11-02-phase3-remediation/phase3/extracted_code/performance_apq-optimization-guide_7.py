# Extracted from: docs/performance/apq-optimization-guide.md
# Block number: 7
# Aggressive caching (5-15 minutes)
apq_backend_config = {"response_ttl": 900}  # 15 minutes

# Moderate caching (1-5 minutes)
apq_backend_config = {"response_ttl": 300}  # 5 minutes

# Short-term caching (30-60 seconds)
apq_backend_config = {"response_ttl": 60}  # 1 minute
