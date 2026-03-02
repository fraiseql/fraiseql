#!/usr/bin/env python3
"""
Users subgraph for fraiseQL federation CI tests.

Serves Federation v2 protocol including _entities, _service { sdl },
introspection, and CRUD for User.  No external database — data is held
in memory so that the container starts instantly and deterministically.
"""
import json
import re
import uuid
from datetime import datetime
from flask import Flask, request, jsonify

app = Flask(__name__)

# ---------------------------------------------------------------------------
# In-memory data store
# ---------------------------------------------------------------------------

USERS = [
    {
        "id": "550e8400-e29b-41d4-a716-446655440001",
        "identifier": "alice@example.com",
        "name": "Alice Johnson",
        "email": "alice@example.com",
        "createdAt": "2024-01-01T00:00:00",
    },
    {
        "id": "550e8400-e29b-41d4-a716-446655440002",
        "identifier": "bob@example.com",
        "name": "Bob Smith",
        "email": "bob@example.com",
        "createdAt": "2024-01-02T00:00:00",
    },
    {
        "id": "550e8400-e29b-41d4-a716-446655440003",
        "identifier": "carol@example.com",
        "name": "Carol White",
        "email": "carol@example.com",
        "createdAt": "2024-01-03T00:00:00",
    },
]

SERVICE_SDL = """
extend schema @link(url: "https://specs.apollo.dev/federation/v2.0")

type User @key(fields: "id") {
  id: ID!
  identifier: String!
  name: String!
  email: String!
  createdAt: String!
}

type Query {
  user(id: ID!): User
  users(limit: Int): [User!]!
}

type Mutation {
  createUser(identifier: String!, name: String!, email: String!): User!
  updateUser(id: ID!, name: String!): User!
  verifyUserExists(userId: ID!): User!
}
""".strip()

# Combined SDL returned when the gateway proxies _service { sdl } queries.
# Contains all entity types with @key so federation tooling can inspect them.
COMBINED_SDL = """
extend schema @link(url: "https://specs.apollo.dev/federation/v2.0")

type User @key(fields: "id") {
  id: ID!
  identifier: String!
  name: String!
  email: String!
  createdAt: String!
  orders(limit: Int): [Order!]!
}

type Order @key(fields: "id") {
  id: ID!
  userId: ID!
  status: String!
  total: Float!
  createdAt: String!
  user: User!
  products(limit: Int): [Product!]!
}

type Product @key(fields: "id") {
  id: ID!
  name: String!
  price: Float!
  stock: Int!
}
""".strip()

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def to_dict(u):
    return {
        "__typename": "User",
        "id": u["id"],
        "identifier": u["identifier"],
        "name": u["name"],
        "email": u["email"],
        "createdAt": u.get("createdAt", ""),
    }


def get_limit(query, variables):
    var_match = re.search(r'users\s*\([^)]*limit\s*:\s*\$(\w+)', query)
    if var_match:
        return variables.get(var_match.group(1))
    lit_match = re.search(r'users\s*\([^)]*limit\s*:\s*(\d+)', query)
    if lit_match:
        return int(lit_match.group(1))
    return None


def find_user(user_id):
    return next((u for u in USERS if u["id"] == user_id), None)


# ---------------------------------------------------------------------------
# Handlers
# ---------------------------------------------------------------------------

def handle_entities(variables):
    representations = variables.get("representations", [])
    results = []
    for rep in representations:
        if rep.get("__typename") == "User":
            user = find_user(rep.get("id", ""))
            results.append(to_dict(user) if user else None)
    return jsonify({"data": {"_entities": results}})


def handle_service():
    return jsonify({"data": {"_service": {"sdl": COMBINED_SDL}}})


def handle_typename():
    return jsonify({"data": {"__typename": "Query"}})


def handle_schema_introspection():
    types = [
        {"name": "Query", "kind": "OBJECT"},
        {"name": "Mutation", "kind": "OBJECT"},
        {"name": "User", "kind": "OBJECT"},
        {"name": "String", "kind": "SCALAR"},
        {"name": "ID", "kind": "SCALAR"},
        {"name": "Int", "kind": "SCALAR"},
        {"name": "Boolean", "kind": "SCALAR"},
        {"name": "__Schema", "kind": "OBJECT"},
        {"name": "__Type", "kind": "OBJECT"},
        {"name": "__Field", "kind": "OBJECT"},
        {"name": "__InputValue", "kind": "OBJECT"},
        {"name": "__EnumValue", "kind": "OBJECT"},
        {"name": "__Directive", "kind": "OBJECT"},
        {"name": "__DirectiveLocation", "kind": "ENUM"},
        {"name": "__TypeKind", "kind": "ENUM"},
    ]
    query_fields = [
        {"name": "user", "type": {"name": "User", "kind": "OBJECT"}},
        {"name": "users", "type": {"name": None, "kind": "LIST"}},
        {"name": "_service", "type": {"name": "_Service", "kind": "OBJECT"}},
        {"name": "_entities", "type": {"name": None, "kind": "LIST"}},
    ]
    return jsonify({
        "data": {
            "__schema": {
                "types": types,
                "queryType": {"name": "Query", "fields": query_fields},
                "mutationType": {"name": "Mutation", "fields": []},
                "directives": [
                    {"name": "skip", "locations": ["FIELD"]},
                    {"name": "include", "locations": ["FIELD"]},
                    {"name": "deprecated", "locations": ["FIELD_DEFINITION", "ENUM_VALUE"]},
                    {"name": "key", "locations": ["OBJECT"]},
                ],
            }
        }
    })


def handle_users(query, variables):
    limit = get_limit(query, variables)
    data = [to_dict(u) for u in (USERS[:limit] if limit is not None else USERS)]
    return jsonify({"data": {"users": data}})


def handle_user(query, variables):
    # Try variable first, then literal from query
    user_id = variables.get("id")
    if not user_id:
        m = re.search(r'user\s*\(\s*id\s*:\s*"([^"]+)"', query)
        if m:
            user_id = m.group(1)
    user = find_user(user_id) if user_id else None
    return jsonify({"data": {"user": to_dict(user) if user else None}})


def get_str_arg(query, field, arg):
    """Extract a string literal argument from inline query syntax."""
    m = re.search(rf'{field}\s*\([^)]*{arg}\s*:\s*"([^"]*)"', query)
    return m.group(1) if m else None


def handle_create_user(query, variables):
    identifier = variables.get("identifier") or get_str_arg(query, "createUser", "identifier") or ""
    name = variables.get("name") or get_str_arg(query, "createUser", "name") or ""
    email = variables.get("email") or get_str_arg(query, "createUser", "email") or identifier
    new_user = {
        "id": str(uuid.uuid4()),
        "identifier": identifier,
        "name": name,
        "email": email,
        "createdAt": datetime.utcnow().isoformat(),
    }
    USERS.append(new_user)
    return jsonify({"data": {"createUser": to_dict(new_user)}})


def handle_update_user(variables):
    user_id = variables.get("id")
    name = variables.get("name")
    user = find_user(user_id)
    if not user:
        return jsonify({
            "data": None,
            "errors": [{"message": f"User {user_id} not found"}],
        })
    user["name"] = name
    return jsonify({"data": {"updateUser": to_dict(user)}})


def handle_verify_user(variables):
    user_id = variables.get("userId")
    user = find_user(user_id)
    if not user:
        return jsonify({
            "data": None,
            "errors": [{"message": f"User {user_id} not found"}],
        })
    return jsonify({"data": {"verifyUserExists": to_dict(user)}})


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
    # Match __schema or __type(...) but NOT __typename in selection sets
    if re.search(r'\b__schema\b|\b__type\s*[\({]', query):
        return handle_schema_introspection()
    if re.search(r'\b__typename\b', query) and not re.search(r'\b(users|user)\b', query):
        return handle_typename()
    if "createUser" in query:
        return handle_create_user(query, variables)
    if "updateUser" in query:
        return handle_update_user(variables)
    if "verifyUserExists" in query:
        return handle_verify_user(variables)
    # `users` before `user` to avoid prefix match issue
    if re.search(r'\busers\b', query):
        return handle_users(query, variables)
    if re.search(r'\buser\b', query):
        return handle_user(query, variables)

    return jsonify({"errors": [{"message": "Unknown operation"}]}), 400


@app.route("/health", methods=["GET"])
def health():
    return jsonify({"status": "healthy"})


if __name__ == "__main__":
    app.run(host="0.0.0.0", port=4000, debug=False)
