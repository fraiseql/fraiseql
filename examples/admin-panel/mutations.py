"""Admin Panel Mutation Resolvers.

Mutation resolvers for admin actions with automatic audit logging.
"""

from uuid import UUID

import fraiseql
from fraiseql import Info
from fraiseql.auth import requires_role

from .models import (
    CustomerInfo,
    CustomerUpdateInput,
    Deal,
    DealUpdateInput,
    Order,
    OrderStatusUpdateInput,
    SupportTicket,
)


async def log_admin_action(
    info: Info,
    action: str,
    target_type: str,
    target_id: UUID,
    details: dict
) -> None:
    """Log admin action to audit trail.

    Args:
        info: GraphQL context
        action: Action type
        target_type: Target entity type
        target_id: Target entity ID
        details: Action details (before/after, reason, etc.)
    """
    await info.context.repo.create(
        "admin_audit_log",
        {
            "admin_user_id": info.context.user["id"],
            "action": action,
            "target_type": target_type,
            "target_id": target_id,
            "details": details,
            "ip_address": info.context.request.client.host if hasattr(info.context, "request") else None,
        },
    )


@fraiseql.mutation
@requires_role("admin")
async def update_customer_status(
    info: Info,
    customer_id: UUID,
    new_status: str,
    reason: str
) -> CustomerInfo:
    """Update customer subscription status.

    Requires admin role. Action is automatically logged to audit trail.

    Args:
        customer_id: Customer to update
        new_status: New subscription status
        reason: Reason for status change

    Returns:
        Updated customer information
    """
    # Get current status for audit log
    customer = await info.context.repo.find_one("customers", customer_id)
    old_status = customer["subscription_status"]

    # Log the action
    await log_admin_action(
        info,
        action="update_customer_status",
        target_type="customer",
        target_id=customer_id,
        details={
            "old_status": old_status,
            "new_status": new_status,
            "reason": reason,
        },
    )

    # Perform update
    updated = await info.context.repo.update(
        "customers", customer_id, {"subscription_status": new_status}
    )

    # Return as CustomerInfo type
    return await info.context.repo.find_one("customer_admin_view", customer_id)


@fraiseql.mutation
@requires_role("customer_support", "admin")
async def update_ticket_status(
    info: Info,
    ticket_id: UUID,
    new_status: str,
    resolution_notes: str | None = None
) -> SupportTicket:
    """Update support ticket status.

    Args:
        ticket_id: Ticket to update
        new_status: New ticket status
        resolution_notes: Optional resolution notes

    Returns:
        Updated ticket
    """
    # Get current ticket for audit
    ticket = await info.context.repo.find_one("support_tickets", ticket_id)

    # Log action
    await log_admin_action(
        info,
        action="update_ticket_status",
        target_type="support_ticket",
        target_id=ticket_id,
        details={
            "old_status": ticket["status"],
            "new_status": new_status,
            "resolution_notes": resolution_notes,
        },
    )

    # Update ticket
    update_data = {"status": new_status}
    if resolution_notes:
        update_data["resolution_notes"] = resolution_notes

    await info.context.repo.update("support_tickets", ticket_id, update_data)

    return await info.context.repo.find_one("support_tickets_view", ticket_id)


@fraiseql.mutation
@requires_role("customer_support", "admin")
async def assign_ticket(
    info: Info,
    ticket_id: UUID,
    assigned_to_id: UUID
) -> SupportTicket:
    """Assign support ticket to an agent.

    Args:
        ticket_id: Ticket to assign
        assigned_to_id: Agent to assign ticket to

    Returns:
        Updated ticket
    """
    # Log action
    await log_admin_action(
        info,
        action="assign_ticket",
        target_type="support_ticket",
        target_id=ticket_id,
        details={
            "assigned_to_id": str(assigned_to_id),
            "assigned_by": str(info.context.user["id"]),
        },
    )

    # Assign ticket
    await info.context.repo.update(
        "support_tickets", ticket_id, {"assigned_to_id": assigned_to_id}
    )

    return await info.context.repo.find_one("support_tickets_view", ticket_id)


@fraiseql.mutation
@requires_role("operations", "admin")
async def update_order_status(
    info: Info,
    input: OrderStatusUpdateInput
) -> Order:
    """Update order fulfillment status.

    Args:
        input: Order status update input

    Returns:
        Updated order
    """
    # Get current order for audit
    order = await info.context.repo.find_one("orders", input.order_id)

    # Log action
    await log_admin_action(
        info,
        action="update_order_status",
        target_type="order",
        target_id=input.order_id,
        details={
            "old_status": order["status"],
            "new_status": input.new_status,
            "notes": input.notes,
        },
    )

    # Update order
    await info.context.repo.update(
        "orders", input.order_id, {"status": input.new_status}
    )

    return await info.context.repo.find_one("orders_view", input.order_id)


@fraiseql.mutation
@requires_role("operations", "admin")
async def mark_order_shipped(
    info: Info,
    order_id: UUID,
    tracking_number: str
) -> Order:
    """Mark order as shipped with tracking information.

    Args:
        order_id: Order to update
        tracking_number: Shipping tracking number

    Returns:
        Updated order
    """
    from datetime import datetime

    # Log action
    await log_admin_action(
        info,
        action="mark_order_shipped",
        target_type="order",
        target_id=order_id,
        details={
            "tracking_number": tracking_number,
            "shipped_at": datetime.now().isoformat(),
        },
    )

    # Update order
    await info.context.repo.update(
        "orders",
        order_id,
        {
            "status": "shipped",
            "shipped_at": datetime.now(),
            "tracking_number": tracking_number,
        },
    )

    return await info.context.repo.find_one("orders_view", order_id)


@fraiseql.mutation
@requires_role("sales", "admin")
async def update_deal_stage(
    info: Info,
    input: DealUpdateInput
) -> Deal:
    """Update deal stage in sales pipeline.

    Args:
        input: Deal update input

    Returns:
        Updated deal
    """
    # Get current deal for audit
    deal = await info.context.repo.find_one("deals", input.deal_id)

    # Prepare update data
    update_data = {}
    if input.stage:
        update_data["stage"] = input.stage
    if input.amount is not None:
        update_data["amount"] = input.amount
    if input.probability is not None:
        update_data["probability"] = input.probability
    if input.expected_close_date:
        update_data["expected_close_date"] = input.expected_close_date

    # Log action
    await log_admin_action(
        info,
        action="update_deal_stage",
        target_type="deal",
        target_id=input.deal_id,
        details={
            "old_stage": deal.get("stage"),
            "updates": update_data,
            "notes": input.notes,
        },
    )

    # Update deal
    await info.context.repo.update("deals", input.deal_id, update_data)

    # If deal moved to closed_won, trigger celebration notification
    if input.stage == "closed_won":
        # In production, this would send Slack notification, update CRM, etc.
        pass

    return await info.context.repo.find_one("deals_view", input.deal_id)


@fraiseql.mutation
@requires_role("sales", "admin")
async def create_deal(
    info: Info,
    company_name: str,
    contact_name: str,
    contact_email: str,
    amount: float,
    expected_close_date: str,
) -> Deal:
    """Create new sales deal.

    Args:
        company_name: Company name
        contact_name: Primary contact name
        contact_email: Primary contact email
        amount: Deal value
        expected_close_date: Expected close date

    Returns:
        Created deal
    """
    from datetime import datetime
    from uuid import uuid4

    deal_id = uuid4()

    # Create deal
    deal = await info.context.repo.create(
        "deals",
        {
            "id": deal_id,
            "company_name": company_name,
            "contact_name": contact_name,
            "contact_email": contact_email,
            "stage": "prospecting",
            "amount": amount,
            "probability": 10,  # Initial probability
            "expected_close_date": datetime.fromisoformat(expected_close_date),
            "assigned_to_id": info.context.user["id"],
        },
    )

    # Log action
    await log_admin_action(
        info,
        action="create_deal",
        target_type="deal",
        target_id=deal_id,
        details={
            "company_name": company_name,
            "amount": amount,
        },
    )

    return await info.context.repo.find_one("deals_view", deal_id)


@fraiseql.mutation
@requires_role("admin")
async def refund_order(
    info: Info,
    order_id: UUID,
    reason: str,
    partial_amount: float | None = None
) -> Order:
    """Process order refund.

    Requires admin role for security.

    Args:
        order_id: Order to refund
        reason: Refund reason
        partial_amount: Optional partial refund amount

    Returns:
        Updated order
    """
    # Get order for audit
    order = await info.context.repo.find_one("orders", order_id)

    # Log action (critical for financial audit)
    await log_admin_action(
        info,
        action="refund_order",
        target_type="order",
        target_id=order_id,
        details={
            "original_total": str(order["total"]),
            "refund_amount": str(partial_amount) if partial_amount else str(order["total"]),
            "reason": reason,
        },
    )

    # Update order status
    await info.context.repo.update(
        "orders",
        order_id,
        {
            "status": "refunded",
            "refund_reason": reason,
            "refund_amount": partial_amount if partial_amount else order["total"],
        },
    )

    # In production: process payment refund via Stripe/PayPal

    return await info.context.repo.find_one("orders_view", order_id)
