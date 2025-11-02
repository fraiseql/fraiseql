# Extracted from: docs/production/observability.md
# Block number: 12
await tracker.capture_exception(
    error,
    context={
        "user_id": user.id,
        "tenant_id": tenant.id,
        "request_id": request_id,
        "operation": operation_name,
        "input_size": len(input_data),
        "database_pool_size": pool.get_size(),
        "memory_usage_mb": get_memory_usage(),
        # Business context
        "order_id": order_id,
        "payment_amount": amount,
        "payment_method": method,
    },
)
