# Extracted from: docs/advanced/authentication.md
# Block number: 21
from fraiseql import query


@query
async def get_cart(info) -> Cart:
    """Get user's shopping cart from session."""
    user = info.context["user"]
    session = info.context.get("session", {})

    cart_id = session.get(f"cart:{user.user_id}")
    if not cart_id:
        # Create new cart
        cart = await create_cart(user.user_id)
        session[f"cart:{user.user_id}"] = cart.id
    else:
        cart = await fetch_cart(cart_id)

    return cart
