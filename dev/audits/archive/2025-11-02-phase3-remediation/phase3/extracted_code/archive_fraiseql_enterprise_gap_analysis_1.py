# Extracted from: docs/archive/fraiseql_enterprise_gap_analysis.md
# Block number: 1
# Proposed API
@requires_permission("user:create", scope="organization")
@attribute_policy("department == user.department")
async def create_user(info, input: CreateUserInput) -> User:
    # Implementation
    pass
