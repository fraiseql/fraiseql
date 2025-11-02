# Extracted from: docs/production/security.md
# Block number: 10
from starlette.middleware.cors import CORSMiddleware

app.add_middleware(
    CORSMiddleware,
    allow_origins=["https://app.yourapp.com", "https://www.yourapp.com"],
    allow_credentials=True,
    allow_methods=["GET", "POST", "OPTIONS"],
    allow_headers=["Content-Type", "Authorization", "X-Request-ID", "X-Correlation-ID"],
    expose_headers=["X-Request-ID"],
    max_age=3600,  # Cache preflight for 1 hour
)
