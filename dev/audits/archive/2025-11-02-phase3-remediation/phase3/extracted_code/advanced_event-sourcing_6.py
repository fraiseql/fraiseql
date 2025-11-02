# Extracted from: docs/advanced/event-sourcing.md
# Block number: 6
# Write Model (Command Side)
class OrderCommandHandler:
    """Handle order commands, generate events."""

    async def create_order(self, customer_id: str) -> str:
        """Create order - generates OrderCreated event."""
        order_id = str(uuid4())

        async with get_db_pool().connection() as conn:
            await conn.execute(
                """
                INSERT INTO orders.orders (id, customer_id, total, status)
                VALUES ($1, $2, 0, 'draft')
            """,
                order_id,
                customer_id,
            )

        # Event automatically logged via trigger
        return order_id

    async def add_item(self, order_id: str, product_id: str, quantity: int, price: Decimal):
        """Add item - generates ItemAdded event."""
        async with get_db_pool().connection() as conn:
            await conn.execute(
                """
                INSERT INTO orders.order_items (id, order_id, product_id, quantity, price, total)
                VALUES ($1, $2, $3, $4, $5, $6)
            """,
                str(uuid4()),
                order_id,
                product_id,
                quantity,
                price,
                price * quantity,
            )

            # Update order total
            await conn.execute(
                """
                UPDATE orders.orders
                SET total = (
                    SELECT SUM(total) FROM orders.order_items WHERE order_id = $1
                )
                WHERE id = $1
            """,
                order_id,
            )


# Read Model (Query Side)
class OrderQueryModel:
    """Optimized read model for order queries."""

    async def get_order_summary(self, order_id: str) -> dict:
        """Get denormalized order summary."""
        async with get_db_pool().connection() as conn:
            result = await conn.execute(
                """
                SELECT
                    o.id,
                    o.customer_id,
                    o.total,
                    o.status,
                    o.created_at,
                    COUNT(oi.id) as item_count,
                    json_agg(
                        json_build_object(
                            'product_id', oi.product_id,
                            'quantity', oi.quantity,
                            'price', oi.price
                        )
                    ) as items
                FROM orders.orders o
                LEFT JOIN orders.order_items oi ON oi.order_id = o.id
                WHERE o.id = $1
                GROUP BY o.id
            """,
                order_id,
            )

            return dict(await result.fetchone())
