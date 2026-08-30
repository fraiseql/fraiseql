#!/usr/bin/env python3
import json
import re
import uuid
from datetime import datetime
from flask import Flask, request, jsonify
import psycopg2
from psycopg2.extras import RealDictCursor
import os
import logging

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

app = Flask(__name__)

# The Apollo Router renames every subgraph operation to `<Name>__<subgraph>__<n>`
# and drops the `query` keyword from anonymous operations. Both broke the
# substring dispatch these stubs used to do, so they match the selection set
# instead. See root_field().
ROOT_FIELD_RE = re.compile(r"\{\s*([A-Za-z_][A-Za-z0-9_]*)")
ARG_RE_VAR = r'{name}\s*:\s*\$(\w+)'
ARG_RE_LIT = r'{name}\s*:\s*"([^"]*)"'


def root_field(query):
    """The first field of the operation's selection set.

    Substring-matching the whole document does not work against a real router.
    Two measured reasons (#1259):

    1. The router renames each subgraph operation to `<Name>__<subgraph>__<n>`,
       so every document this service receives carries the subgraph's own name.
       A dispatcher testing `'user' in query` therefore matched `user(id:)` and
       `users` alike and answered the list for both — the router found no `user`
       field in the reply and returned `{"data":{"user":null}}` with no error.
    2. The router omits the `query` keyword for an anonymous operation, so
       `{ users { id } }` arrives with no `query` in it. A dispatcher requiring
       that keyword rejected this example's own first query as "Unknown query".

    Reading the first field of the selection set answers what was actually
    asked. Aliases are not resolved; these are stubs, not a GraphQL engine.
    """
    m = ROOT_FIELD_RE.search(query)
    return m.group(1) if m else ""


def arg(query, variables, name, default=None):
    """Resolve a GraphQL argument from the request.

    Reading `variables[name]` alone is silently wrong whenever the caller names
    the variable something other than the argument: the default is used, no
    error is raised, and the operation looks like it succeeded on inputs nobody
    sent. So prefer a variable named for the argument, then follow `name: $var`
    to the variable it names, then take an inline literal.
    """
    if name in variables:
        return variables[name]
    m = re.search(ARG_RE_VAR.format(name=re.escape(name)), query)
    if m and m.group(1) in variables:
        return variables[m.group(1)]
    m = re.search(ARG_RE_LIT.format(name=re.escape(name)), query)
    if m:
        return m.group(1)
    return default


def get_db_connection(database='fraiseql_inventory'):
    """Get database connection.

    The inventory subgraph owns `fraiseql_inventory`, separate from the orders
    and users stores — no transaction spans them, which is the whole reason the
    saga compensates rather than rolls back.
    """
    try:
        conn = psycopg2.connect(
            host='postgres',
            database=database,
            user='fraiseql',
            password='fraiseql123'
        )
        return conn
    except Exception as e:
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

    field = root_field(query)

    try:
        if field == 'reserveItems':
            return handle_reserve_items(query, variables)
        elif field == 'releaseReservation':
            return handle_release_reservation(query, variables)
        elif field == 'reservation':
            return handle_get_reservation(query, variables)
        elif field == 'product':
            return handle_get_product(query, variables)
        elif field == '__typename':
            return jsonify({"data": {"__typename": "Query"}})
        else:
            return jsonify({"errors": [{"message": f"inventory does not serve '{field}'"}]}), 400
    except Exception as e:
        logger.error(f"Query execution error: {e}")
        return jsonify({"errors": [{"message": str(e)}]}), 500

def handle_reserve_items(query, variables):
    """Reserve items from inventory (saga step 3)"""
    items = arg(query, variables, 'items', [])
    order_id = arg(query, variables, 'orderId')

    logger.info(f"Reserving items for order {order_id}")

    try:
        reservation_id = str(uuid.uuid4())

        conn = get_db_connection()
        cur = conn.cursor()

        # Check if items are in stock
        for item in items:
            cur.execute(
                'SELECT stock FROM tb_product WHERE id = %s',
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
            INSERT INTO tb_reservation (id, order_id, status)
            VALUES (%s, %s, %s)
        ''', (reservation_id, order_id, 'reserved'))

        # Create reservation items and decrease stock
        for item in items:
            cur.execute('''
                INSERT INTO tb_reservation_item (id, fk_reservation, product_id, quantity)
                SELECT %s, pk_reservation, %s, %s FROM tb_reservation WHERE id = %s
            ''', (str(uuid.uuid4()), item['productId'], item['quantity'], reservation_id))

            cur.execute('''
                UPDATE tb_product SET stock = stock - %s WHERE id = %s
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

def handle_release_reservation(query, variables):
    """Release reservation (compensation)"""
    reservation_id = arg(query, variables, 'reservationId')

    logger.info(f"Releasing reservation: {reservation_id}")

    try:
        conn = get_db_connection()
        cur = conn.cursor()

        # Get reservation items
        cur.execute('''
            SELECT i.product_id, i.quantity
              FROM tb_reservation_item i
              JOIN tb_reservation r ON r.pk_reservation = i.fk_reservation
             WHERE r.id = %s
        ''', (reservation_id,))
        items = cur.fetchall()

        # Restore stock
        for product_id, quantity in items:
            cur.execute('''
                UPDATE tb_product SET stock = stock + %s WHERE id = %s
            ''', (quantity, product_id))

        # Update reservation status
        cur.execute('''
            UPDATE tb_reservation SET status = %s WHERE id = %s
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

def handle_get_reservation(query, variables):
    """Get single reservation"""
    reservation_id = arg(query, variables, 'id')

    try:
        conn = get_db_connection()
        cur = conn.cursor(cursor_factory=RealDictCursor)

        cur.execute('SELECT * FROM tb_reservation WHERE id = %s', (reservation_id,))
        reservation = cur.fetchone()

        if not reservation:
            cur.close()
            conn.close()
            return jsonify({
                "data": {"reservation": None},
                "errors": [{"message": f"Reservation {reservation_id} not found"}]
            }), 404

        cur.execute('''
            SELECT i.product_id, i.quantity
              FROM tb_reservation_item i
              JOIN tb_reservation r ON r.pk_reservation = i.fk_reservation
             WHERE r.id = %s
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

def handle_get_product(query, variables):
    """Get product"""
    product_id = arg(query, variables, 'id')

    try:
        conn = get_db_connection()
        cur = conn.cursor(cursor_factory=RealDictCursor)

        cur.execute('SELECT * FROM tb_product WHERE id = %s', (product_id,))
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
