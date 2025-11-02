# Extracted from: docs/rust/RUST_PIPELINE_IMPLEMENTATION_GUIDE.md
# Block number: 1
from fraiseql.db import FraiseQLRepository

repo = FraiseQLRepository(pool)

# List queries - use find_rust
users = await repo.find_rust("v_user", "users", info)

# Single object queries - use find_one_rust
user = await repo.find_one_rust("v_user", "user", info, id=user_id)

# With filtering
active_users = await repo.find_rust(
    "v_user", "users", info, status="active", created_at__min="2024-01-01"
)
