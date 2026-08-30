#!/usr/bin/env python3
import json
import re
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


def get_db_connection():
    """Get database connection"""
    try:
        conn = psycopg2.connect(
            host='postgres',
            database='fraiseql',
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
        if field == 'verifyUserExists':
            return handle_verify_user(query, variables)
        elif field == 'users':
            return handle_get_users()
        elif field == 'user':
            return handle_get_user(query, variables)
        elif field == '__typename':
            return jsonify({"data": {"__typename": "Query"}})
        else:
            return jsonify({"errors": [{"message": f"users does not serve '{field}'"}]}), 400
    except Exception as e:
        logger.error(f"Query execution error: {e}")
        return jsonify({"errors": [{"message": str(e)}]}), 500

def handle_verify_user(query, variables):
    """Verify user exists (saga step)"""
    user_id = arg(query, variables, 'userId')
    logger.info(f"Verifying user: {user_id}")

    try:
        conn = get_db_connection()
        cur = conn.cursor(cursor_factory=RealDictCursor)
        cur.execute('SELECT * FROM tb_user WHERE id = %s', (user_id,))
        user = cur.fetchone()
        cur.close()
        conn.close()

        if not user:
            return jsonify({
                "data": None,
                "errors": [{"message": f"User {user_id} not found"}]
            }), 404

        return jsonify({
            "data": {
                "verifyUserExists": {
                    "id": str(user['id']),
                    "name": user['name'],
                    "email": user['email'],
                    "createdAt": user['created_at'].isoformat()
                }
            }
        })
    except Exception as e:
        logger.error(f"Error verifying user: {e}")
        return jsonify({
            "data": None,
            "errors": [{"message": str(e)}]
        }), 500

def handle_get_user(query, variables):
    """Get single user"""
    user_id = arg(query, variables, 'id')

    try:
        conn = get_db_connection()
        cur = conn.cursor(cursor_factory=RealDictCursor)
        cur.execute('SELECT * FROM tb_user WHERE id = %s', (user_id,))
        user = cur.fetchone()
        cur.close()
        conn.close()

        if not user:
            return jsonify({
                "data": {"user": None},
                "errors": [{"message": f"User {user_id} not found"}]
            }), 404

        return jsonify({
            "data": {
                "user": {
                    "id": str(user['id']),
                    "name": user['name'],
                    "email": user['email'],
                    "createdAt": user['created_at'].isoformat()
                }
            }
        })
    except Exception as e:
        logger.error(f"Error getting user: {e}")
        return jsonify({
            "data": None,
            "errors": [{"message": str(e)}]
        }), 500

def handle_get_users():
    """Get all users"""
    try:
        conn = get_db_connection()
        cur = conn.cursor(cursor_factory=RealDictCursor)
        cur.execute('SELECT * FROM tb_user ORDER BY created_at DESC')
        users = cur.fetchall()
        cur.close()
        conn.close()

        return jsonify({
            "data": {
                "users": [
                    {
                        "id": str(u['id']),
                        "name": u['name'],
                        "email": u['email'],
                        "createdAt": u['created_at'].isoformat()
                    }
                    for u in users
                ]
            }
        })
    except Exception as e:
        logger.error(f"Error getting users: {e}")
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
