# Extracted from: docs/advanced/authentication.md
# Block number: 17
from fraiseql.auth import InMemoryRevocationStore, RevocationConfig, TokenRevocationService

# Create revocation store
revocation_store = InMemoryRevocationStore()

# Create revocation service
revocation_service = TokenRevocationService(
    store=revocation_store,
    config=RevocationConfig(
        enabled=True,
        check_revocation=True,
        ttl=86400,  # 24 hours
        cleanup_interval=3600,  # Clean expired every hour
    ),
)

# Start cleanup task
await revocation_service.start()
