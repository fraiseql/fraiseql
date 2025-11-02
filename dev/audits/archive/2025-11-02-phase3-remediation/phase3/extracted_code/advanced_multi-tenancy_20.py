# Extracted from: docs/advanced/multi-tenancy.md
# Block number: 20
# Small tenants: Shared pool
config = FraiseQLConfig(database_pool_size=20, database_max_overflow=10)

# Large tenant: Dedicated pool
large_tenant_pool = DatabasePool(
    "postgresql://user:pass@localhost/tenant_large", min_size=10, max_size=30
)
