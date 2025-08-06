"""Test cart operations for the e-commerce API
"""

from decimal import Decimal

import pytest


@pytest.mark.asyncio
async def test_add_to_cart(test_client, sample_product_variant):
    """Test adding items to cart"""
    query = """
    mutation AddToCart($variantId: UUID!, $quantity: Int!) {
        addToCart(variantId: $variantId, quantity: $quantity) {
            success
            message
            cartId
            cart {
                id
                itemCount
                totalQuantity
                subtotal
                items
            }
        }
    }
    """

    variables = {"variantId": str(sample_product_variant["id"]), "quantity": 2}

    response = await test_client.post(
        "/graphql", json={"query": query, "variables": variables},
    )

    assert response.status_code == 200
    data = response.json()["data"]["addToCart"]

    assert data["success"] is True
    assert data["cartId"] is not None
    assert data["cart"]["itemCount"] == 1
    assert data["cart"]["totalQuantity"] == 2
    assert Decimal(data["cart"]["subtotal"]) == Decimal("119.98")  # 59.99 * 2


@pytest.mark.asyncio
async def test_update_cart_item_quantity(test_client, cart_with_items):
    """Test updating cart item quantity"""
    cart_item = cart_with_items["items"][0]

    query = """
    mutation UpdateCartItem($cartItemId: UUID!, $quantity: Int!) {
        updateCartItem(cartItemId: $cartItemId, quantity: $quantity) {
            success
            message
            cart {
                id
                totalQuantity
                subtotal
            }
        }
    }
    """

    variables = {"cartItemId": cart_item["id"], "quantity": 5}

    response = await test_client.post(
        "/graphql", json={"query": query, "variables": variables},
    )

    assert response.status_code == 200
    data = response.json()["data"]["updateCartItem"]

    assert data["success"] is True
    assert data["cart"]["totalQuantity"] == 5


@pytest.mark.asyncio
async def test_remove_cart_item(test_client, cart_with_items):
    """Test removing item from cart"""
    cart_item = cart_with_items["items"][0]

    query = """
    mutation RemoveCartItem($cartItemId: UUID!) {
        updateCartItem(cartItemId: $cartItemId, quantity: 0) {
            success
            message
            cartId
        }
    }
    """

    variables = {"cartItemId": cart_item["id"]}

    response = await test_client.post(
        "/graphql", json={"query": query, "variables": variables},
    )

    assert response.status_code == 200
    data = response.json()["data"]["updateCartItem"]

    assert data["success"] is True
    assert data["message"] == "Item removed from cart"


@pytest.mark.asyncio
async def test_apply_coupon(test_client, cart_with_items, valid_coupon):
    """Test applying coupon to cart"""
    query = """
    mutation ApplyCoupon($cartId: UUID!, $couponCode: String!) {
        applyCouponToCart(cartId: $cartId, couponCode: $couponCode) {
            success
            message
            discountAmount
            cart {
                id
                subtotal
            }
        }
    }
    """

    variables = {"cartId": cart_with_items["id"], "couponCode": valid_coupon["code"]}

    response = await test_client.post(
        "/graphql", json={"query": query, "variables": variables},
    )

    assert response.status_code == 200
    data = response.json()["data"]["applyCouponToCart"]

    assert data["success"] is True
    assert data["discountAmount"] is not None
    assert Decimal(data["discountAmount"]) > 0


@pytest.mark.asyncio
async def test_cart_inventory_validation(test_client, low_stock_variant):
    """Test cart validates inventory availability"""
    query = """
    mutation AddToCart($variantId: UUID!, $quantity: Int!) {
        addToCart(variantId: $variantId, quantity: $quantity) {
            success
            error
        }
    }
    """

    # Try to add more than available
    variables = {
        "variantId": str(low_stock_variant["id"]),
        "quantity": 1000,  # More than available
    }

    response = await test_client.post(
        "/graphql", json={"query": query, "variables": variables},
    )

    assert response.status_code == 200
    data = response.json()["data"]["addToCart"]

    assert data["success"] is False
    assert "Insufficient inventory" in data["error"]


@pytest.mark.asyncio
async def test_clear_cart(test_client, cart_with_items):
    """Test clearing all items from cart"""
    query = """
    mutation ClearCart($cartId: UUID!) {
        clearCart(cartId: $cartId) {
            success
            message
            cartId
        }
    }
    """

    variables = {"cartId": cart_with_items["id"]}

    response = await test_client.post(
        "/graphql", json={"query": query, "variables": variables},
    )

    assert response.status_code == 200
    data = response.json()["data"]["clearCart"]

    assert data["success"] is True
    assert data["message"] == "Cart cleared"
