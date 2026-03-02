#!/usr/bin/env python3
"""
Products subgraph for fraiseQL federation CI tests.

Provides Product entity CRUD with in-memory storage.
"""
import json
import re
from flask import Flask, request, jsonify

app = Flask(__name__)

# ---------------------------------------------------------------------------
# In-memory data store
# ---------------------------------------------------------------------------

PRODUCTS = [
    {"id": "prod-001", "name": "Laptop", "price": 999.99, "stock": 50},
    {"id": "prod-002", "name": "Mouse", "price": 29.99, "stock": 200},
    {"id": "prod-003", "name": "Keyboard", "price": 79.99, "stock": 150},
    {"id": "prod-004", "name": "Monitor", "price": 399.99, "stock": 30},
    {"id": "prod-005", "name": "Headphones", "price": 149.99, "stock": 75},
]

SERVICE_SDL = """
extend schema @link(url: "https://specs.apollo.dev/federation/v2.0")

type Product @key(fields: "id") {
  id: ID!
  name: String!
  price: Float!
  stock: Int!
}

type Query {
  product(id: ID!): Product
  products(limit: Int): [Product!]!
}
""".strip()

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def to_dict(p):
    return {
        "__typename": "Product",
        "id": p["id"],
        "name": p["name"],
        "price": float(p["price"]),
        "stock": p["stock"],
    }


def get_limit(query, variables):
    var_match = re.search(r'products\s*\([^)]*limit\s*:\s*\$(\w+)', query)
    if var_match:
        return variables.get(var_match.group(1))
    lit_match = re.search(r'products\s*\([^)]*limit\s*:\s*(\d+)', query)
    if lit_match:
        return int(lit_match.group(1))
    return None


def find_product(product_id):
    return next((p for p in PRODUCTS if p["id"] == product_id), None)


# ---------------------------------------------------------------------------
# Handlers
# ---------------------------------------------------------------------------

def handle_entities(variables):
    representations = variables.get("representations", [])
    results = []
    for rep in representations:
        if rep.get("__typename") == "Product":
            prod = find_product(rep.get("id", ""))
            results.append(to_dict(prod) if prod else None)
    return jsonify({"data": {"_entities": results}})


def handle_service():
    return jsonify({"data": {"_service": {"sdl": SERVICE_SDL}}})


def handle_typename():
    return jsonify({"data": {"__typename": "Query"}})


def handle_schema_introspection():
    types = [
        {"name": "Query", "kind": "OBJECT"},
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
                    {"name": "product", "type": {"name": "Product", "kind": "OBJECT"}},
                    {"name": "products", "type": {"name": None, "kind": "LIST"}},
                ]},
                "mutationType": None,
                "directives": [],
            }
        }
    })


def handle_products(query, variables):
    limit = get_limit(query, variables)
    data = [to_dict(p) for p in (PRODUCTS[:limit] if limit is not None else PRODUCTS)]
    return jsonify({"data": {"products": data}})


def handle_product(query, variables):
    product_id = variables.get("id")
    if not product_id:
        m = re.search(r'product\s*\(\s*id\s*:\s*"([^"]+)"', query)
        if m:
            product_id = m.group(1)
    prod = find_product(product_id) if product_id else None
    return jsonify({"data": {"product": to_dict(prod) if prod else None}})


# ---------------------------------------------------------------------------
# Router
# ---------------------------------------------------------------------------

@app.route("/graphql", methods=["POST"])
def graphql():
    body = request.get_json(force=True) or {}
    query = body.get("query", "")
    variables = body.get("variables") or {}

    if "_entities" in query:
        return handle_entities(variables)
    if "_service" in query:
        return handle_service()
    if re.search(r'\b__schema\b|\b__type\s*[\({]', query):
        return handle_schema_introspection()
    if re.search(r'\b__typename\b', query) and not re.search(r'\bproduct', query):
        return handle_typename()
    # `products` before `product` to avoid prefix issue
    if re.search(r'\bproducts\b', query):
        return handle_products(query, variables)
    if re.search(r'\bproduct\b', query):
        return handle_product(query, variables)

    return jsonify({"errors": [{"message": "Unknown operation"}]}), 400


@app.route("/health", methods=["GET"])
def health():
    return jsonify({"status": "healthy"})


if __name__ == "__main__":
    app.run(host="0.0.0.0", port=4000, debug=False)
