#!/usr/bin/env python3
import json
import uuid
from datetime import datetime
from flask import Flask, request, jsonify
import psycopg2
from psycopg2.extras import RealDictCursor
import logging

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

app = Flask(__name__)

def get_db_connection():
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

def log_audit_event(transaction_id, event_type, details):
    """Log audit trail"""
    try:
        conn = get_db_connection()
        cur = conn.cursor()
        cur.execute(
            'INSERT INTO audit_log (transaction_id, event_type, details) VALUES (%s, %s, %s)',
            (transaction_id, event_type, json.dumps(details))
        )
        conn.commit()
        cur.close()
        conn.close()
    except Exception as e:
        logger.error(f"Failed to log audit event: {e}")

@app.route('/graphql', methods=['POST'])
def graphql():
    data = request.get_json()
    query = data.get('query', '')
    variables = data.get('variables', {})

    try:
        if 'transferMoney' in query:
            return handle_transfer_money(variables)
        elif 'account' in query:
            return handle_get_account(variables)
        elif 'compensateTransfer' in query:
            return handle_compensate_transfer(variables)
        else:
            return jsonify({"errors": [{"message": "Unknown query"}]}), 400
    except Exception as e:
        logger.error(f"Query error: {e}")
        return jsonify({"errors": [{"message": str(e)}]}), 500

def handle_transfer_money(variables):
    """Execute transfer with manual compensation capability"""
    from_account_id = variables.get('fromAccountId')
    to_account_id = variables.get('toAccountId')
    amount = variables.get('amount')
    transaction_id = variables.get('transactionId')

    logger.info(f"Transfer: {from_account_id} -> {to_account_id}, Amount: {amount}, TxnId: {transaction_id}")

    try:
        conn = get_db_connection()
        cur = conn.cursor(cursor_factory=RealDictCursor)

        # Check idempotency - has this transfer been processed?
        cur.execute('SELECT * FROM transfers WHERE transaction_id = %s', (transaction_id,))
        existing = cur.fetchone()
        if existing:
            log_audit_event(transaction_id, 'IDEMPOTENT_RETRY', {'previous_status': existing['status']})
            cur.close()
            conn.close()
            return jsonify({
                "data": {
                    "transferMoney": {
                        "transactionId": transaction_id,
                        "status": existing['status'],
                        "message": "Transfer already processed"
                    }
                }
            })

        # Verify both accounts exist
        cur.execute('SELECT * FROM accounts WHERE id = %s FOR UPDATE', (from_account_id,))
        from_account = cur.fetchone()

        if not from_account:
            log_audit_event(transaction_id, 'TRANSFER_FAILED', {'reason': 'From account not found'})
            cur.close()
            conn.close()
            return jsonify({"errors": [{"message": f"Account {from_account_id} not found"}]}), 404

        if from_account['status'] != 'active':
            log_audit_event(transaction_id, 'TRANSFER_FAILED', {'reason': f'From account {from_account["status"]}'})
            cur.close()
            conn.close()
            return jsonify({"errors": [{"message": f"Account is {from_account['status']}"}]}), 400

        cur.execute('SELECT * FROM accounts WHERE id = %s FOR UPDATE', (to_account_id,))
        to_account = cur.fetchone()

        if not to_account:
            log_audit_event(transaction_id, 'TRANSFER_FAILED', {'reason': 'To account not found'})
            cur.close()
            conn.close()
            return jsonify({"errors": [{"message": f"Account {to_account_id} not found"}]}), 404

        if to_account['status'] != 'active':
            log_audit_event(transaction_id, 'TRANSFER_FAILED', {'reason': f'To account {to_account["status"]}'})
            cur.close()
            conn.close()
            return jsonify({"errors": [{"message": f"Receiver account is {to_account['status']}"}]}), 400

        # Check sufficient funds
        if from_account['balance'] < amount:
            log_audit_event(transaction_id, 'TRANSFER_FAILED', {'reason': 'Insufficient funds'})
            cur.close()
            conn.close()
            return jsonify({"errors": [{"message": "Insufficient funds"}]}), 400

        # Execute transfer atomically
        cur.execute(
            'UPDATE accounts SET balance = balance - %s WHERE id = %s',
            (amount, from_account_id)
        )
        cur.execute(
            'UPDATE accounts SET balance = balance + %s WHERE id = %s',
            (amount, to_account_id)
        )

        # Record transfer
        cur.execute(
            '''INSERT INTO transfers (transaction_id, from_account_id, to_account_id, amount, status)
               VALUES (%s, %s, %s, %s, %s)''',
            (transaction_id, from_account_id, to_account_id, amount, 'completed')
        )

        conn.commit()

        log_audit_event(transaction_id, 'TRANSFER_COMPLETED', {
            'from_account': from_account_id,
            'to_account': to_account_id,
            'amount': float(amount)
        })

        return jsonify({
            "data": {
                "transferMoney": {
                    "transactionId": transaction_id,
                    "status": "completed",
                    "fromBalance": float(from_account['balance'] - amount),
                    "toBalance": float(to_account['balance'] + amount)
                }
            }
        })

    except Exception as e:
        logger.error(f"Transfer error: {e}")
        log_audit_event(transaction_id, 'TRANSFER_ERROR', {'error': str(e)})
        return jsonify({"errors": [{"message": str(e)}]}), 500
    finally:
        cur.close()
        conn.close()

def handle_compensate_transfer(variables):
    """Manual compensation - return funds if transfer failed downstream"""
    transaction_id = variables.get('transactionId')

    logger.info(f"Compensating transfer: {transaction_id}")

    try:
        conn = get_db_connection()
        cur = conn.cursor(cursor_factory=RealDictCursor)

        # Get transfer
        cur.execute('SELECT * FROM transfers WHERE transaction_id = %s', (transaction_id,))
        transfer = cur.fetchone()

        if not transfer:
            cur.close()
            conn.close()
            return jsonify({"errors": [{"message": f"Transfer {transaction_id} not found"}]}), 404

        # Check if already compensated
        cur.execute('SELECT * FROM compensation_records WHERE transaction_id = %s', (transaction_id,))
        existing_comp = cur.fetchone()
        if existing_comp:
            cur.close()
            conn.close()
            return jsonify({
                "data": {
                    "compensateTransfer": {
                        "transactionId": transaction_id,
                        "status": "already_compensated"
                    }
                }
            })

        # Return funds from receiver to sender
        cur.execute(
            'UPDATE accounts SET balance = balance - %s WHERE id = %s',
            (transfer['amount'], transfer['to_account_id'])
        )
        cur.execute(
            'UPDATE accounts SET balance = balance + %s WHERE id = %s',
            (transfer['amount'], transfer['from_account_id'])
        )

        # Record compensation
        cur.execute(
            'INSERT INTO compensation_records (transaction_id, compensation_type, status) VALUES (%s, %s, %s)',
            (transaction_id, 'RETURN_FUNDS', 'completed')
        )

        conn.commit()

        log_audit_event(transaction_id, 'TRANSFER_COMPENSATED', {'action': 'Funds returned'})

        return jsonify({
            "data": {
                "compensateTransfer": {
                    "transactionId": transaction_id,
                    "status": "compensated"
                }
            }
        })

    except Exception as e:
        logger.error(f"Compensation error: {e}")
        log_audit_event(transaction_id, 'COMPENSATION_FAILED', {'error': str(e)})
        return jsonify({"errors": [{"message": str(e)}]}), 500
    finally:
        cur.close()
        conn.close()

def handle_get_account(variables):
    """Get account balance"""
    account_id = variables.get('accountId')

    try:
        conn = get_db_connection()
        cur = conn.cursor(cursor_factory=RealDictCursor)
        cur.execute('SELECT * FROM accounts WHERE id = %s', (account_id,))
        account = cur.fetchone()
        cur.close()
        conn.close()

        if not account:
            return jsonify({"data": {"account": None}}), 404

        return jsonify({
            "data": {
                "account": {
                    "id": account['id'],
                    "accountNumber": account['account_number'],
                    "accountHolder": account['account_holder'],
                    "balance": float(account['balance']),
                    "status": account['status']
                }
            }
        })
    except Exception as e:
        logger.error(f"Error getting account: {e}")
        return jsonify({"errors": [{"message": str(e)}]}), 500

@app.route('/health', methods=['GET'])
def health():
    try:
        conn = get_db_connection()
        conn.close()
        return jsonify({"status": "healthy"})
    except Exception as e:
        return jsonify({"status": "unhealthy", "error": str(e)}), 500

if __name__ == '__main__':
    app.run(host='0.0.0.0', port=4000, debug=False)
