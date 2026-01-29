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

def get_db_connection(database='fraiseql_inventory'):
    """Get database connection"""
    try:
        conn = mysql.connector.connect(
            host='mysql',
            database=database,
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
        if 'reserveItems' in query:
            return handle_reserve_items(variables)
        elif 'releaseReservation' in query:
            return handle_release_reservation(variables)
        elif 'reservation' in query and 'id' in variables:
            return handle_get_reservation(variables)
        elif 'product' in query:
            return handle_get_product(variables)
        else:
            return jsonify({"errors": [{"message": "Unknown query"}]}), 400
    except Exception as e:
        logger.error(f"Query execution error: {e}")
        return jsonify({"errors": [{"message": str(e)}]}), 500

def handle_reserve_items(variables):
    """Reserve items from inventory (saga step 3)"""
    items = variables.get('items', [])
    order_id = variables.get('orderId')

    logger.info(f"Reserving items for order {order_id}")

    try:
        reservation_id = str(uuid.uuid4())

        conn = get_db_connection()
        cur = conn.cursor()

        # Check if items are in stock
        for item in items:
            cur.execute(
                'SELECT stock FROM products WHERE id = %s',
                (item['productId'],)
            )
            result = cur.fetchone()
            if not result or result[0] < item['quantity']:
                cur.close()
                conn.close()
                return jsonify({
                    "data": None,
                    "errors": [{"message": f"Insufficient stock for product {item['productId']}"}]
                }), 400

        # Create reservation
        cur.execute('''
            INSERT INTO reservations (id, order_id, status)
            VALUES (%s, %s, %s)
        ''', (reservation_id, order_id, 'reserved'))

        # Create reservation items and decrease stock
        for item in items:
            cur.execute('''
                INSERT INTO reservation_items (id, reservation_id, product_id, quantity)
                VALUES (%s, %s, %s, %s)
            ''', (str(uuid.uuid4()), reservation_id, item['productId'], item['quantity']))

            cur.execute('''
                UPDATE products SET stock = stock - %s WHERE id = %s
            ''', (item['quantity'], item['productId']))

        conn.commit()
        cur.close()
        conn.close()

        return jsonify({
            "data": {
                "reserveItems": {
                    "id": reservation_id,
                    "orderId": order_id,
                    "status": "reserved",
                    "items": [
                        {
                            "productId": item['productId'],
                            "quantity": item['quantity']
                        }
                        for item in items
                    ],
                    "createdAt": datetime.now().isoformat()
                }
            }
        })
    except Exception as e:
        logger.error(f"Error reserving items: {e}")
        return jsonify({
            "data": None,
            "errors": [{"message": str(e)}]
        }), 500

def handle_release_reservation(variables):
    """Release reservation (compensation)"""
    reservation_id = variables.get('reservationId')

    logger.info(f"Releasing reservation: {reservation_id}")

    try:
        conn = get_db_connection()
        cur = conn.cursor()

        # Get reservation items
        cur.execute('''
            SELECT product_id, quantity FROM reservation_items WHERE reservation_id = %s
        ''', (reservation_id,))
        items = cur.fetchall()

        # Restore stock
        for product_id, quantity in items:
            cur.execute('''
                UPDATE products SET stock = stock + %s WHERE id = %s
            ''', (quantity, product_id))

        # Update reservation status
        cur.execute('''
            UPDATE reservations SET status = %s WHERE id = %s
        ''', ('released', reservation_id))

        conn.commit()
        cur.close()
        conn.close()

        return jsonify({
            "data": {
                "releaseReservation": {
                    "id": reservation_id,
                    "status": "released"
                }
            }
        })
    except Exception as e:
        logger.error(f"Error releasing reservation: {e}")
        return jsonify({
            "data": None,
            "errors": [{"message": str(e)}]
        }), 500

def handle_get_reservation(variables):
    """Get single reservation"""
    reservation_id = variables.get('id')

    try:
        conn = get_db_connection()
        cur = conn.cursor(dictionary=True)

        cur.execute('SELECT * FROM reservations WHERE id = %s', (reservation_id,))
        reservation = cur.fetchone()

        if not reservation:
            cur.close()
            conn.close()
            return jsonify({
                "data": {"reservation": None},
                "errors": [{"message": f"Reservation {reservation_id} not found"}]
            }), 404

        cur.execute('''
            SELECT product_id, quantity FROM reservation_items WHERE reservation_id = %s
        ''', (reservation_id,))
        items = cur.fetchall()

        cur.close()
        conn.close()

        return jsonify({
            "data": {
                "reservation": {
                    "id": reservation['id'],
                    "orderId": reservation['order_id'],
                    "status": reservation['status'],
                    "items": [
                        {
                            "productId": item['product_id'],
                            "quantity": item['quantity']
                        }
                        for item in items
                    ],
                    "createdAt": reservation['created_at'].isoformat() if hasattr(reservation['created_at'], 'isoformat') else str(reservation['created_at'])
                }
            }
        })
    except Exception as e:
        logger.error(f"Error getting reservation: {e}")
        return jsonify({
            "data": None,
            "errors": [{"message": str(e)}]
        }), 500

def handle_get_product(variables):
    """Get product"""
    product_id = variables.get('id')

    try:
        conn = get_db_connection()
        cur = conn.cursor(dictionary=True)

        cur.execute('SELECT * FROM products WHERE id = %s', (product_id,))
        product = cur.fetchone()

        cur.close()
        conn.close()

        if not product:
            return jsonify({
                "data": {"product": None},
                "errors": [{"message": f"Product {product_id} not found"}]
            }), 404

        return jsonify({
            "data": {
                "product": {
                    "id": product['id'],
                    "name": product['name'],
                    "stock": product['stock'],
                    "price": float(product['price'])
                }
            }
        })
    except Exception as e:
        logger.error(f"Error getting product: {e}")
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
