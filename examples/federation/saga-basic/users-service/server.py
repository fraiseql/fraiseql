#!/usr/bin/env python3
import json
from datetime import datetime
from flask import Flask, request, jsonify
import psycopg2
from psycopg2.extras import RealDictCursor
import os
import logging

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

app = Flask(__name__)

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

    try:
        # Parse the query to determine operation
        if 'verifyUserExists' in query:
            return handle_verify_user(variables)
        elif 'query' in query and 'user' in query:
            if 'userId' in query:
                return handle_get_user(variables)
            else:
                return handle_get_users()
        else:
            return jsonify({"errors": [{"message": "Unknown query"}]}), 400
    except Exception as e:
        logger.error(f"Query execution error: {e}")
        return jsonify({"errors": [{"message": str(e)}]}), 500

def handle_verify_user(variables):
    """Verify user exists (saga step)"""
    user_id = variables.get('userId')
    logger.info(f"Verifying user: {user_id}")

    try:
        conn = get_db_connection()
        cur = conn.cursor(cursor_factory=RealDictCursor)
        cur.execute('SELECT * FROM users WHERE id = %s', (user_id,))
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

def handle_get_user(variables):
    """Get single user"""
    user_id = variables.get('id')

    try:
        conn = get_db_connection()
        cur = conn.cursor(cursor_factory=RealDictCursor)
        cur.execute('SELECT * FROM users WHERE id = %s', (user_id,))
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
        cur.execute('SELECT * FROM users ORDER BY created_at DESC')
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
