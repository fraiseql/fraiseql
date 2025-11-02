# Extracted from: docs/performance/index.md
# Block number: 5
from fraiseql import FraiseQLRepository

repo = FraiseQLRepository(
    pool,
    pool_size=20,  # Adjust based on load
    max_overflow=10,
)
