# Extracted from: docs/enterprise/RBAC_POSTGRESQL_ASSESSMENT.md
# Block number: 5
# Store permissions with version metadata
versions = await cache.get_domain_versions(
    tenant_id, ["role", "permission", "role_permission", "user_role"]
)

await cache.set(
    key=f"rbac:permissions:{user_id}:{tenant_id}",
    value=permissions,
    ttl=300,  # 5 minutes
    versions=versions,  # Attach version metadata
)

# On retrieval, versions are checked automatically
# If any domain version changed, cache is stale (returns None)
result, cached_versions = await cache.get_with_metadata(cache_key)

current_versions = await cache.get_domain_versions(tenant_id, domains)
if cached_versions and cached_versions != current_versions:
    # Cache stale - recompute permissions
    permissions = await compute_permissions(user_id, tenant_id)
