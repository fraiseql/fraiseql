# Extracted from: docs/production/observability.md
# Block number: 7
from opentelemetry import trace

from fraiseql import query

tracer = trace.get_tracer(__name__)


@query
async def get_user_orders(info, user_id: str) -> list[Order]:
    # Create span
    with tracer.start_as_current_span(
        "get_user_orders", attributes={"user.id": user_id, "operation.type": "query"}
    ) as span:
        # Database query
        with tracer.start_as_current_span("db.query") as db_span:
            db_span.set_attribute("db.statement", "SELECT * FROM v_order WHERE user_id = $1")
            db_span.set_attribute("db.system", "postgresql")

            orders = await info.context["repo"].find("v_order", where={"user_id": user_id})

            db_span.set_attribute("db.rows_returned", len(orders))

        # Add business context
        span.set_attribute("orders.count", len(orders))
        span.set_attribute("orders.total_value", sum(o.total for o in orders))

        return orders
