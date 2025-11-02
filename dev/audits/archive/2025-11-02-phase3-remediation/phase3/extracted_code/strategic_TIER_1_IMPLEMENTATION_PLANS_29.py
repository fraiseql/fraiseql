# Extracted from: docs/strategic/TIER_1_IMPLEMENTATION_PLANS.md
# Block number: 29
class RoleHierarchy:
    """Role hierarchy engine with cycle detection and caching."""

    def __init__(self, repo: FraiseQLRepository):
        self.repo = repo
        self._hierarchy_cache: dict[UUID, List[Role]] = {}

    async def get_inherited_roles(self, role_id: UUID, use_cache: bool = True) -> List[Role]:
        """Get inherited roles with caching.

        Args:
            role_id: Starting role
            use_cache: Whether to use cache

        Returns:
            List of roles in inheritance order

        Raises:
            ValueError: If cycle detected
        """
        if use_cache and role_id in self._hierarchy_cache:
            return self._hierarchy_cache[role_id]

        # Use PostgreSQL recursive CTE (handles cycles with depth limit)
        results = await self.repo.run(
            DatabaseQuery(
                statement="SELECT * FROM get_inherited_roles(%s)",
                params={"role_id": str(role_id)},
                fetch_result=True,
            )
        )

        if not results:
            return []

        # Check if we hit cycle detection limit (depth = 10)
        if any(r["depth"] >= 10 for r in results):
            raise ValueError(f"Cycle detected in role hierarchy for role {role_id}")

        # Get full role details
        role_ids = [r["role_id"] for r in results]
        roles_data = await self.repo.run(
            DatabaseQuery(
                statement="""
                SELECT * FROM roles
                WHERE id = ANY(%s::uuid[])
            """,
                params={"ids": role_ids},
                fetch_result=True,
            )
        )

        roles = [Role(**row) for row in roles_data]

        # Cache result
        self._hierarchy_cache[role_id] = roles

        return roles

    def clear_cache(self, role_id: Optional[UUID] = None):
        """Clear hierarchy cache.

        Args:
            role_id: If provided, clear only this role. Otherwise clear all.
        """
        if role_id:
            self._hierarchy_cache.pop(role_id, None)
        else:
            self._hierarchy_cache.clear()
