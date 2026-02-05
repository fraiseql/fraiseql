#!/usr/bin/env python3
import json
import uuid
from datetime import datetime
from flask import Flask, request, jsonify
import mysql.connector
from mysql.connector import Error
import os
import logging

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

app = Flask(__name__)

def get_db_connection():
    """Get database connection"""
    try:
        conn = mysql.connector.connect(
            host='mysql',
            database='fraiseql',
            user='fraiseql',
            password='fraiseql123'
        )
        return conn
    except Error as e:
        logger.error(f"Database connection failed: {e}")
        raise

@app.route('/graphql', methods=['POST'])
def graphql():
    """Handle GraphQL queries and mutations"""
    data = request.get_json()
    query = data.get('query', '')
    variables = data.get('variables', {})

    logger.info(f"GraphQL Query: {query}")
    logger.info(f"Variables: {variables}")

    try:
        # Parse the query to determine operation
        if 'createOrder' in query:
            return handle_create_order(variables)
        elif 'cancelOrder' in query:
            return handle_cancel_order(variables)
        elif 'ordersByUser' in query:
            return handle_get_orders_by_user(variables)
        elif 'order' in query and 'id' in variables:
            return handle_get_order(variables)
        else:
            return jsonify({"errors": [{"message": "Unknown query"}]}), 400
    except Exception as e:
        logger.error(f"Query execution error: {e}")
        return jsonify({"errors": [{"message": str(e)}]}), 500

def handle_create_order(variables):
    """Create order (saga step 4)"""
    user_id = variables.get('userId')
    items = variables.get('items', [])
    charge_id = variables.get('chargeId')
    reservation_id = variables.get('reservationId')

    logger.info(f"Creating order for user {user_id}")

    try:
        order_id = str(uuid.uuid4())
        total = sum(item['price'] * item['quantity'] for item in items)

        conn = get_db_connection()
        cur = conn.cursor()

        # Create order
        cur.execute('''
            INSERT INTO orders (id, user_id, status, total, created_at)
            VALUES (%s, %s, %s, %s, NOW())
        ''', (order_id, user_id, 'confirmed', total))

        # Insert order items
        for item in items:
            cur.execute('''
                INSERT INTO order_items (id, order_id, product_id, quantity, price)
                VALUES (%s, %s, %s, %s, %s)
            ''', (str(uuid.uuid4()), order_id, item['productId'], item['quantity'], item['price']))

        conn.commit()
        cur.close()
        conn.close()

        return jsonify({
            "data": {
                "createOrder": {
                    "id": order_id,
                    "userId": user_id,
                    "status": "confirmed",
                    "total": total,
                    "items": [
                        {
                            "productId": item['productId'],
                            "quantity": item['quantity'],
                            "price": item['price']
                        }
                        for item in items
                    ],
                    "createdAt": datetime.now().isoformat()
                }
            }
        })
    except Exception as e:
        logger.error(f"Error creating order: {e}")
        return jsonify({
            "data": None,
            "errors": [{"message": str(e)}]
        }), 500

def handle_cancel_order(variables):
    """Cancel order (compensation)"""
    order_id = variables.get('orderId')

    logger.info(f"Cancelling order: {order_id}")

    try:
        conn = get_db_connection()
        cur = conn.cursor()

        cur.execute('''
            UPDATE orders SET status = %s WHERE id = %s
        ''', ('cancelled', order_id))

        conn.commit()
        cur.close()
        conn.close()

        return jsonify({
            "data": {
                "cancelOrder": {
                    "id": order_id,
                    "status": "cancelled"
                }
            }
        })
    except Exception as e:
        logger.error(f"Error cancelling order: {e}")
        return jsonify({
            "data": None,
            "errors": [{"message": str(e)}]
        }), 500

def handle_get_order(variables):
    """Get single order"""
    order_id = variables.get('id')

    try:
        conn = get_db_connection()
        cur = conn.cursor(dictionary=True)

        cur.execute('SELECT * FROM orders WHERE id = %s', (order_id,))
        order = cur.fetchone()

        if not order:
            cur.close()
            conn.close()
            return jsonify({
                "data": {"order": None},
                "errors": [{"message": f"Order {order_id} not found"}]
            }), 404

        cur.execute('SELECT * FROM order_items WHERE order_id = %s', (order_id,))
        items = cur.fetchall()

        cur.close()
        conn.close()

        return jsonify({
            "data": {
                "order": {
                    "id": order['id'],
                    "userId": order['user_id'],
                    "status": order['status'],
                    "total": float(order['total']),
                    "items": [
                        {
                            "productId": item['product_id'],
                            "quantity": item['quantity'],
                            "price": float(item['price'])
                        }
                        for item in items
                    ],
                    "createdAt": order['created_at'].isoformat() if hasattr(order['created_at'], 'isoformat') else str(order['created_at'])
                }
            }
        })
    except Exception as e:
        logger.error(f"Error getting order: {e}")
        return jsonify({
            "data": None,
            "errors": [{"message": str(e)}]
        }), 500

def handle_get_orders_by_user(variables):
    """Get orders by user"""
    user_id = variables.get('userId')

    try:
        conn = get_db_connection()
        cur = conn.cursor(dictionary=True)

        cur.execute('SELECT * FROM orders WHERE user_id = %s ORDER BY created_at DESC', (user_id,))
        orders = cur.fetchall()

        result_orders = []
        for order in orders:
            cur.execute('SELECT * FROM order_items WHERE order_id = %s', (order['id'],))
            items = cur.fetchall()

            result_orders.append({
                "id": order['id'],
                "userId": order['user_id'],
                "status": order['status'],
                "total": float(order['total']),
                "items": [
                    {
                        "productId": item['product_id'],
                        "quantity": item['quantity'],
                        "price": float(item['price'])
                    }
                    for item in items
                ],
                "createdAt": order['created_at'].isoformat() if hasattr(order['created_at'], 'isoformat') else str(order['created_at'])
            })

        cur.close()
        conn.close()

        return jsonify({
            "data": {"ordersByUser": result_orders}
        })
    except Exception as e:
        logger.error(f"Error getting orders: {e}")
        return jsonify({
            "data": None,
            "errors": [{"message": str(e)}]
        }), 500

@app.route('/health', methods=['GET'])
def health():
    """Health check endpoint"""
    try:
        conn = get_db_connection()
        conn.close()
        return jsonify({"status": "healthy"})
    except Exception as e:
        return jsonify({"status": "unhealthy", "error": str(e)}), 500

if __name__ == '__main__':
    app.run(host='0.0.0.0', port=4000, debug=False)
