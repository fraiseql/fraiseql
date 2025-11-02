# Extracted from: docs/performance/apq-optimization-guide.md
# Block number: 6
from fraiseql.middleware.apq_caching import handle_apq_request_with_cache

# Add tenant context
context = {"tenant_id": request.headers.get("X-Tenant-ID")}

cached_response = handle_apq_request_with_cache(
    request=graphql_request,
    backend=backend,
    config=config,
    context=context,  # Tenant-specific caching
)
