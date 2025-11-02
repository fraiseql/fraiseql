# Extracted from: docs/performance/caching.md
# Block number: 26
import logging

# Enable cache logging
logging.getLogger("fraiseql.caching").setLevel(logging.INFO)

# Logs include:
# - Extension detection: "✓ Detected pg_fraiseql_cache v1.0.0"
# - Cache initialization: "PostgreSQL cache table 'fraiseql_cache' initialized"
# - Cleanup operations: "Cleaned 145 expired cache entries"
# - Errors: "Failed to get cache key 'fraiseql:...' ..."
