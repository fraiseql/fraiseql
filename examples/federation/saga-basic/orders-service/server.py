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


# Items join through the surrogate key, not the UUID — `tb_order_item.fk_order`
# references `tb_order.pk_order` (the Trinity pattern).
ORDER_ITEMS_SQL = '''
    SELECT i.*
      FROM tb_order_item i
      JOIN tb_order o ON o.pk_order = i.fk_order
     WHERE o.id = %s
'''

def get_db_connection():
    """Get database connection.

    The orders subgraph owns `fraiseql_orders`; it cannot see the users or
    inventory tables, which is what makes the saga's compensations necessary.
    """
    try:
        conn = psycopg2.connect(
            host='postgres',
            database='fraiseql_orders',
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
        if field == 'createOrder':
            return handle_create_order(query, variables)
        elif field == 'cancelOrder':
            return handle_cancel_order(query, variables)
        elif field == 'ordersByUser':
            return handle_get_orders_by_user(query, variables)
        elif field == 'order':
            return handle_get_order(query, variables)
        elif field == '__typename':
            return jsonify({"data": {"__typename": "Query"}})
        else:
            return jsonify({"errors": [{"message": f"orders does not serve '{field}'"}]}), 400
    except Exception as e:
        logger.error(f"Query execution error: {e}")
        return jsonify({"errors": [{"message": str(e)}]}), 500

def handle_create_order(query, variables):
    """Create order (saga step 4)"""
    user_id = arg(query, variables, 'userId')
    items = arg(query, variables, 'items', [])
    charge_id = arg(query, variables, 'chargeId')
    reservation_id = arg(query, variables, 'reservationId')

    logger.info(f"Creating order for user {user_id}")

    try:
        order_id = str(uuid.uuid4())
        total = sum(item['price'] * item['quantity'] for item in items)

        conn = get_db_connection()
        cur = conn.cursor()

        # Create order
        cur.execute('''
            INSERT INTO tb_order (id, user_id, status, total, created_at)
            VALUES (%s, %s, %s, %s, NOW())
        ''', (order_id, user_id, 'confirmed', total))

        # Insert order items
        for item in items:
            cur.execute('''
                INSERT INTO tb_order_item (id, fk_order, product_id, quantity, price)
                SELECT %s, pk_order, %s, %s, %s FROM tb_order WHERE id = %s
            ''', (str(uuid.uuid4()), item['productId'], item['quantity'], item['price'], order_id))

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

def handle_cancel_order(query, variables):
    """Cancel order (compensation)"""
    order_id = arg(query, variables, 'orderId')

    logger.info(f"Cancelling order: {order_id}")

    try:
        conn = get_db_connection()
        cur = conn.cursor()

        cur.execute('''
            UPDATE tb_order SET status = %s WHERE id = %s
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

def handle_get_order(query, variables):
    """Get single order"""
    order_id = arg(query, variables, 'id')

    try:
        conn = get_db_connection()
        cur = conn.cursor(cursor_factory=RealDictCursor)

        cur.execute('SELECT * FROM tb_order WHERE id = %s', (order_id,))
        order = cur.fetchone()

        if not order:
            cur.close()
            conn.close()
            return jsonify({
                "data": {"order": None},
                "errors": [{"message": f"Order {order_id} not found"}]
            }), 404

        cur.execute(ORDER_ITEMS_SQL, (order_id,))
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

def handle_get_orders_by_user(query, variables):
    """Get orders by user"""
    user_id = arg(query, variables, 'userId')

    try:
        conn = get_db_connection()
        cur = conn.cursor(cursor_factory=RealDictCursor)

        cur.execute('SELECT * FROM tb_order WHERE user_id = %s ORDER BY created_at DESC', (user_id,))
        orders = cur.fetchall()

        result_orders = []
        for order in orders:
            cur.execute(ORDER_ITEMS_SQL, (order['id'],))
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
