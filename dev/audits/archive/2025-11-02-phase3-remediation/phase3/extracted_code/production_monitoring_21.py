# Extracted from: docs/production/monitoring.md
# Block number: 21
# Log all SQL queries in development
from fraiseql.fastapi.config import FraiseQLConfig

config = FraiseQLConfig(
    database_url="postgresql://...",
    database_echo=True,  # Development only
)

# Production: Log slow queries only
# PostgreSQL: log_min_duration_statement = 1000  # Log queries > 1s
