"""Admin Panel Query Resolvers.

Query resolvers for customer support, operations, and sales dashboards.
"""

from uuid import UUID

import fraiseql
from fraiseql import Info
from fraiseql.auth import requires_role

from .models import (
    AuditLogEntry,
    CustomerInfo,
    Deal,
    OperationsMetrics,
    Order,
    SalesMetrics,
    SupportTicket,
)


@fraiseql.query
@requires_role("customer_support", "admin")
async def customer_search(
    info: Info, query: str, status: str | None = None, limit: int = 50
) -> list[CustomerInfo]:
    """Search customers by email, name, or ID.

    Args:
        query: Search query (email, name, or ID)
        status: Optional subscription status filter
        limit: Maximum results to return

    Returns:
        List of matching customers
    """
    filters = {"search": query}
    if status:
        filters["subscription_status"] = status

    return await info.context.repo.find("customer_admin_view", where=filters, limit=limit)


@fraiseql.query
@requires_role("customer_support", "admin")
async def customer_by_id(info: Info, customer_id: UUID) -> CustomerInfo | None:
    """Get customer by ID.

    Args:
        customer_id: Customer unique identifier

    Returns:
        Customer information if found
    """
    return await info.context.repo.find_one("customer_admin_view", customer_id)


@fraiseql.query
@requires_role("customer_support", "admin")
async def support_tickets(
    info: Info,
    status: str | None = None,
    priority: str | None = None,
    assigned_to: UUID | None = None,
    limit: int = 50,
) -> list[SupportTicket]:
    """Get support tickets with optional filters.

    Args:
        status: Filter by ticket status
        priority: Filter by priority
        assigned_to: Filter by assigned agent
        limit: Maximum results

    Returns:
        List of support tickets
    """
    filters = {}
    if status:
        filters["status"] = status
    if priority:
        filters["priority"] = priority
    if assigned_to:
        filters["assigned_to_id"] = assigned_to

    return await info.context.repo.find(
        "support_tickets_view", where=filters, limit=limit, order_by="-created_at"
    )


@fraiseql.query
@requires_role("customer_support", "admin")
async def customer_support_tickets(
    info: Info, customer_id: UUID, limit: int = 10
) -> list[SupportTicket]:
    """Get support tickets for a specific customer.

    Args:
        customer_id: Customer ID
        limit: Maximum tickets to return

    Returns:
        Customer's support tickets
    """
    return await info.context.repo.find(
        "support_tickets_view",
        where={"customer_id": customer_id},
        limit=limit,
        order_by="-created_at",
    )


@fraiseql.query
@requires_role("operations", "admin")
async def operations_metrics(info: Info) -> OperationsMetrics:
    """Get real-time operations dashboard metrics.

    Returns:
        Current operations metrics
    """
    # Use pre-computed materialized view for performance
    metrics = await info.context.repo.find_one("operations_metrics_mv")
    return OperationsMetrics(**metrics)


@fraiseql.query
@requires_role("operations", "admin")
async def orders(
    info: Info,
    status: str | None = None,
    customer_id: UUID | None = None,
    limit: int = 50,
) -> list[Order]:
    """Get orders with optional filters.

    Args:
        status: Filter by order status
        customer_id: Filter by customer
        limit: Maximum results

    Returns:
        List of orders
    """
    filters = {}
    if status:
        filters["status"] = status
    if customer_id:
        filters["customer_id"] = customer_id

    return await info.context.repo.find(
        "orders_view", where=filters, limit=limit, order_by="-created_at"
    )


@fraiseql.query
@requires_role("operations", "admin")
async def order_by_id(info: Info, order_id: UUID) -> Order | None:
    """Get order by ID with full details.

    Args:
        order_id: Order unique identifier

    Returns:
        Order details if found
    """
    return await info.context.repo.find_one("orders_view", order_id)


@fraiseql.query
@requires_role("operations", "admin")
async def orders_needing_attention(info: Info, limit: int = 100) -> list[Order]:
    """Get orders that need attention (stuck, delayed, etc.).

    Args:
        limit: Maximum orders to return

    Returns:
        Orders requiring operations team attention
    """
    return await info.context.repo.find(
        "orders_needing_attention_view", limit=limit, order_by="-created_at"
    )


@fraiseql.query
@requires_role("sales", "admin")
async def sales_metrics(info: Info, rep_id: UUID | None = None) -> list[SalesMetrics]:
    """Get sales team or individual rep metrics.

    Args:
        rep_id: Optional sales rep ID (returns all reps if omitted)

    Returns:
        Sales performance metrics
    """
    filters = {}
    if rep_id:
        filters["rep_id"] = rep_id

    return await info.context.repo.find("sales_metrics_view", where=filters)


@fraiseql.query
@requires_role("sales", "admin")
async def deals(
    info: Info,
    stage: str | None = None,
    assigned_to: UUID | None = None,
    limit: int = 100,
) -> list[Deal]:
    """Get deals/opportunities in pipeline.

    Args:
        stage: Filter by deal stage
        assigned_to: Filter by sales rep
        limit: Maximum deals to return

    Returns:
        List of deals
    """
    filters = {}
    if stage:
        filters["stage"] = stage
    if assigned_to:
        filters["assigned_to_id"] = assigned_to

    return await info.context.repo.find(
        "deals_view", where=filters, limit=limit, order_by="-updated_at"
    )


@fraiseql.query
@requires_role("sales", "admin")
async def my_pipeline(info: Info) -> list[Deal]:
    """Get current user's sales pipeline.

    Returns:
        Deals assigned to current user
    """
    user_id = info.context.user["id"]
    return await info.context.repo.find(
        "deals_view",
        where={
            "assigned_to_id": user_id,
            "stage": {"not_in": ["closed_won", "closed_lost"]},
        },
        order_by="-expected_close_date",
    )


@fraiseql.query
@requires_role("admin")
async def audit_log(
    info: Info,
    admin_user_id: UUID | None = None,
    action: str | None = None,
    target_type: str | None = None,
    limit: int = 100,
) -> list[AuditLogEntry]:
    """Get admin action audit log.

    Args:
        admin_user_id: Filter by admin user
        action: Filter by action type
        target_type: Filter by target entity type
        limit: Maximum entries to return

    Returns:
        Audit log entries
    """
    filters = {}
    if admin_user_id:
        filters["admin_user_id"] = admin_user_id
    if action:
        filters["action"] = action
    if target_type:
        filters["target_type"] = target_type

    return await info.context.repo.find(
        "admin_audit_log", where=filters, limit=limit, order_by="-created_at"
    )


@fraiseql.query
@requires_role("admin")
async def audit_log_for_entity(
    info: Info, target_type: str, target_id: UUID, limit: int = 50
) -> list[AuditLogEntry]:
    """Get audit log for a specific entity.

    Args:
        target_type: Entity type (customer, order, deal, etc.)
        target_id: Entity ID
        limit: Maximum entries

    Returns:
        Audit log entries for the entity
    """
    return await info.context.repo.find(
        "admin_audit_log",
        where={"target_type": target_type, "target_id": target_id},
        limit=limit,
        order_by="-created_at",
    )
