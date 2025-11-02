# Extracted from: docs/core/configuration.md
# Block number: 9
# Production CORS (specific origins)
config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    cors_enabled=True,
    cors_origins=["https://app.example.com", "https://admin.example.com"],
    cors_methods=["GET", "POST", "OPTIONS"],
    cors_headers=["Content-Type", "Authorization", "X-Request-ID"],
)

# Development CORS (permissive)
config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    environment="development",
    cors_enabled=True,
    cors_origins=["http://localhost:3000", "http://localhost:8080"],
)
