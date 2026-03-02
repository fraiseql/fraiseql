#!/usr/bin/env python3
"""
Orders subgraph for fraiseQL federation CI tests.

Provides Order CRUD plus entity resolution for:
  - User (extends User with User.orders field)
  - Order (owned entity)
  - Product (reference only, resolvable: false)
"""
import re
import uuid
from datetime import datetime
from flask import Flask, request, jsonify

app = Flask(__name__)

# ---------------------------------------------------------------------------
# In-memory data store
# ---------------------------------------------------------------------------

ORDERS = [
    {
        "id": "order-001",
        "userId": "550e8400-e29b-41d4-a716-446655440001",
        "status": "completed",
        "total": 149.99,
        "productIds": ["prod-001", "prod-002"],
        "createdAt": "2024-01-01T10:00:00",
    },
    {
        "id": "order-002",
        "userId": "550e8400-e29b-41d4-a716-446655440002",
        "status": "pending",
        "total": 29.99,
        "productIds": ["prod-002"],
        "createdAt": "2024-01-02T10:00:00",
    },
    {
        "id": "order-003",
        "userId": "550e8400-e29b-41d4-a716-446655440001",
        "status": "processing",
        "total": 79.99,
        "productIds": ["prod-003"],
        "createdAt": "2024-01-03T10:00:00",
    },
]

SERVICE_SDL = """
extend schema @link(url: "https://specs.apollo.dev/federation/v2.0")

type Order @key(fields: "id") {
  id: ID!
  userId: ID!
  status: String!
  total: Float!
  user: User!
  products(limit: Int): [Product!]!
  createdAt: String!
}

extend type User @key(fields: "id") {
  id: ID! @external
  orders(limit: Int): [Order!]!
}

extend type Product @key(fields: "id") {
  id: ID! @external
}

type Query {
  orders(limit: Int): [Order!]!
  order(id: ID!): Order
  ordersByUser(userId: ID!): [Order!]!
  user(id: ID!): User
}

type Mutation {
  createOrder(userId: ID!, status: String!, total: Float!): Order!
  cancelOrder(orderId: ID!): Order!
}
""".strip()

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def order_to_dict(o, product_limit=None):
    products = o.get("productIds", [])
    if product_limit is not None:
        products = products[:product_limit]
    return {
        "__typename": "Order",
        "id": o["id"],
        "userId": o["userId"],
        "status": o["status"],
        "total": float(o["total"]),
        # user and products are entity references resolved by router
        "user": {"__typename": "User", "id": o["userId"]},
        "products": [{"__typename": "Product", "id": pid} for pid in products],
        "createdAt": o.get("createdAt", ""),
    }


def get_limit(query, variables, field="orders"):
    var_match = re.search(rf'{field}\s*\([^)]*limit\s*:\s*\$(\w+)', query)
    if var_match:
        return variables.get(var_match.group(1))
    lit_match = re.search(rf'{field}\s*\([^)]*limit\s*:\s*(\d+)', query)
    if lit_match:
        return int(lit_match.group(1))
    return None


def get_str_arg(query, field, arg):
    """Extract a string literal argument from inline query syntax."""
    m = re.search(rf'{field}\s*\([^)]*{arg}\s*:\s*"([^"]*)"', query)
    return m.group(1) if m else None


def get_float_arg(query, field, arg):
    """Extract a float literal argument from inline query syntax."""
    m = re.search(rf'{field}\s*\([^)]*{arg}\s*:\s*([\d.]+)', query)
    return float(m.group(1)) if m else None


def find_order(order_id):
    return next((o for o in ORDERS if o["id"] == order_id), None)


def orders_for_user(user_id):
    return [o for o in ORDERS if o["userId"] == user_id]


# ---------------------------------------------------------------------------
# Handlers
# ---------------------------------------------------------------------------

def handle_entities(variables, query=""):
    representations = variables.get("representations", [])
    results = []
    # Extract limit for orders field if present in the query context
    order_limit = get_limit(query, variables, "orders")
    for rep in representations:
        typename = rep.get("__typename")
        if typename == "User":
            uid = rep.get("id", "")
            user_orders = orders_for_user(uid)
            if order_limit is not None:
                user_orders = user_orders[:order_limit]
            results.append({
                "__typename": "User",
                "id": uid,
                "orders": [order_to_dict(o) for o in user_orders],
            })
        elif typename == "Order":
            oid = rep.get("id", "")
            order = find_order(oid)
            results.append(order_to_dict(order) if order else None)
        elif typename == "Product":
            # Products are resolved by the products subgraph; return the key only
            results.append({"__typename": "Product", "id": rep.get("id")})
        else:
            results.append(None)
    return jsonify({"data": {"_entities": results}})


def handle_service():
    return jsonify({"data": {"_service": {"sdl": SERVICE_SDL}}})


def handle_typename():
    return jsonify({"data": {"__typename": "Query"}})


def handle_schema_introspection():
    types = [
        {"name": "Query", "kind": "OBJECT"},
        {"name": "Mutation", "kind": "OBJECT"},
        {"name": "Order", "kind": "OBJECT"},
        {"name": "User", "kind": "OBJECT"},
        {"name": "Product", "kind": "OBJECT"},
        {"name": "String", "kind": "SCALAR"},
        {"name": "ID", "kind": "SCALAR"},
        {"name": "Int", "kind": "SCALAR"},
        {"name": "Float", "kind": "SCALAR"},
        {"name": "Boolean", "kind": "SCALAR"},
    ]
    return jsonify({
        "data": {
            "__schema": {
                "types": types,
                "queryType": {"name": "Query", "fields": [
                    {"name": "orders", "type": {"name": None, "kind": "LIST"}},
                    {"name": "order", "type": {"name": "Order", "kind": "OBJECT"}},
                    {"name": "user", "type": {"name": "User", "kind": "OBJECT"}},
                ]},
                "mutationType": {"name": "Mutation", "fields": []},
                "directives": [],
            }
        }
    })


def handle_orders(query, variables):
    limit = get_limit(query, variables, "orders")
    data = [order_to_dict(o) for o in (ORDERS[:limit] if limit is not None else ORDERS)]
    return jsonify({"data": {"orders": data}})


def handle_order(query, variables):
    order_id = variables.get("id")
    if not order_id:
        m = re.search(r'order\s*\(\s*id\s*:\s*"([^"]+)"', query)
        if m:
            order_id = m.group(1)
    order = find_order(order_id) if order_id else None
    return jsonify({"data": {"order": order_to_dict(order) if order else None}})


def handle_orders_by_user(query, variables):
    user_id = variables.get("userId")
    if not user_id:
        m = re.search(r'ordersByUser\s*\(\s*userId\s*:\s*"([^"]+)"', query)
        if m:
            user_id = m.group(1)
    data = [order_to_dict(o) for o in orders_for_user(user_id or "")]
    return jsonify({"data": {"ordersByUser": data}})


def handle_user(query, variables):
    """Return a minimal User entity (orders subgraph only knows user IDs)."""
    user_id = variables.get("id")
    if not user_id:
        m = re.search(r'user\s*\(\s*id\s*:\s*"([^"]+)"', query)
        if m:
            user_id = m.group(1)
    if not user_id:
        return jsonify({"data": {"user": None}})
    return jsonify({"data": {"user": {"__typename": "User", "id": user_id}}})


def handle_create_order(query, variables):
    user_id = variables.get("userId") or get_str_arg(query, "createOrder", "userId") or ""
    status = variables.get("status") or get_str_arg(query, "createOrder", "status") or "pending"
    total_val = variables.get("total")
    if total_val is None:
        total_val = get_float_arg(query, "createOrder", "total") or 0.0
    total = float(total_val)
    new_order = {
        "id": str(uuid.uuid4()),
        "userId": user_id,
        "status": status,
        "total": total,
        "productIds": [],
        "createdAt": datetime.utcnow().isoformat(),
    }
    ORDERS.append(new_order)
    return jsonify({"data": {"createOrder": order_to_dict(new_order)}})


def handle_cancel_order(query, variables):
    order_id = variables.get("orderId")
    if not order_id:
        m = re.search(r'cancelOrder\s*\(\s*orderId\s*:\s*"([^"]+)"', query)
        if m:
            order_id = m.group(1)
    order = find_order(order_id)
    if not order:
        return jsonify({
            "data": None,
            "errors": [{"message": f"Order {order_id} not found"}],
        })
    order["status"] = "cancelled"
    return jsonify({"data": {"cancelOrder": order_to_dict(order)}})


# ---------------------------------------------------------------------------
# Router
# ---------------------------------------------------------------------------

@app.route("/graphql", methods=["POST"])
def graphql():
    body = request.get_json(force=True) or {}
    query = body.get("query", "")
    variables = body.get("variables") or {}

    if "_entities" in query:
        return handle_entities(variables, query)
    if "_service" in query:
        return handle_service()
    if re.search(r'\b__schema\b|\b__type\s*[\({]', query):
        return handle_schema_introspection()
    if re.search(r'\b__typename\b', query) and not re.search(r'\border', query):
        return handle_typename()
    if "createOrder" in query:
        return handle_create_order(query, variables)
    if "cancelOrder" in query:
        return handle_cancel_order(query, variables)
    if "ordersByUser" in query:
        return handle_orders_by_user(query, variables)
    if re.search(r'\borders\b', query):
        return handle_orders(query, variables)
    if re.search(r'\border\b', query):
        return handle_order(query, variables)
    if re.search(r'\buser\b', query):
        return handle_user(query, variables)

    return jsonify({"errors": [{"message": "Unknown operation"}]}), 400


@app.route("/health", methods=["GET"])
def health():
    return jsonify({"status": "healthy"})


if __name__ == "__main__":
    app.run(host="0.0.0.0", port=4000, debug=False)
