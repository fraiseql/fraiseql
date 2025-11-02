# Extracted from: docs/migration/v0-to-v1.md
# Block number: 3
from fraiseql import type


@type
class User:
    id: UUID  # Now properly handled
    email: EmailStr  # Email validation
    ip_address: IPAddress  # Network types
    created_at: datetime  # Timezone-aware
