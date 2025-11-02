# Extracted from: docs/production/security.md
# Block number: 9
from fraiseql.fastapi.config import FraiseQLConfig

config = FraiseQLConfig(
    database_url="postgresql://...",
    # CORS - disabled by default, configure explicitly
    cors_enabled=True,
    cors_origins=[
        "https://app.yourapp.com",
        "https://www.yourapp.com",
        # NEVER use "*" in production
    ],
    cors_methods=["GET", "POST"],
    cors_headers=["Content-Type", "Authorization", "X-Request-ID"],
)
