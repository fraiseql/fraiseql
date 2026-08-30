#!/usr/bin/env python3
import json
import re
import uuid
from decimal import Decimal
from datetime import datetime
from flask import Flask, request, jsonify
import psycopg2
from psycopg2.extras import RealDictCursor
import logging

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

app = Flask(__name__)

# The Apollo Router renames every subgraph operation to `<Name>__<subgraph>__<n>`
# and drops the `query` keyword from anonymous operations, so a dispatcher that
# substring-matches the document reads the wrong thing. These stubs match the
# selection set instead. See root_field().
ROOT_FIELD_RE = re.compile(r"\{\s*([A-Za-z_][A-Za-z0-9_]*)")
ARG_RE_VAR = r'{name}\s*:\s*\$(\w+)'
ARG_RE_STR = r'{name}\s*:\s*"([^"]*)"'
ARG_RE_NUM = r'{name}\s*:\s*(-?\d+(?:\.\d+)?)(?![\w.])'


def root_field(query):
    """The first field of the operation's selection set.

    Substring-matching the whole document is not safe against a real router: it
    renames each subgraph operation to `<Name>__<subgraph>__<n>`, and it omits
    the `query` keyword for anonymous operations. Reading the first field of the
    selection set answers what was actually asked. Aliases are not resolved;
    this is a stub, not a GraphQL engine.
    """
    m = ROOT_FIELD_RE.search(query)
    return m.group(1) if m else ""


def arg(query, variables, name, default=None):
    """Resolve a GraphQL argument from the request.

    Reading `variables[name]` alone is silently wrong twice over. It misses a
    caller who names the variable something other than the argument, and it
    misses inline literals entirely — every query in this example's own
    `test-saga.sh` passes literals and sends no variables at all, so every
    handler here resolved every argument to None and PostgreSQL was asked
    `WHERE id = NULL` (#1259). So: prefer a variable named for the argument,
    then follow `name: $var` to the variable it names, then take a literal.
    """
    if name in variables:
        return variables[name]
    m = re.search(ARG_RE_VAR.format(name=re.escape(name)), query)
    if m and m.group(1) in variables:
        return variables[m.group(1)]
    m = re.search(ARG_RE_STR.format(name=re.escape(name)), query)
    if m:
        return m.group(1)
    m = re.search(ARG_RE_NUM.format(name=re.escape(name)), query)
    if m:
        raw = m.group(1)
        return float(raw) if '.' in raw else int(raw)
    return default


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
            'INSERT INTO tb_audit_log (transaction_id, event_type, details) VALUES (%s, %s, %s)',
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

    field = root_field(query)

    try:
        if field == 'transferMoney':
            return handle_transfer_money(query, variables)
        elif field == 'compensateTransfer':
            return handle_compensate_transfer(query, variables)
        elif field == 'account':
            return handle_get_account(query, variables)
        elif field == '__typename':
            return jsonify({"data": {"__typename": "Query"}})
        else:
            return jsonify({"errors": [{"message": f"bank does not serve '{field}'"}]}), 400
    except Exception as e:
        logger.error(f"Query error: {e}")
        return jsonify({"errors": [{"message": str(e)}]}), 500

def handle_transfer_money(query, variables):
    """Execute a transfer, recording it so it can be compensated later."""
    from_account_id = arg(query, variables, 'fromAccountId')
    to_account_id = arg(query, variables, 'toAccountId')
    amount = arg(query, variables, 'amount')
    transaction_id = arg(query, variables, 'transactionId')

    if amount is None:
        return jsonify({"errors": [{"message": "amount is required"}]}), 400
    # Balances are DECIMAL(15,2). Subtracting a float from a Decimal raises
    # TypeError, so normalise once here rather than at each arithmetic site.
    amount = Decimal(str(amount))

    logger.info(f"Transfer: {from_account_id} -> {to_account_id}, Amount: {amount}, TxnId: {transaction_id}")

    # Bound before the try: the previous version closed them in `finally`, so a
    # connection failure raised NameError from the cleanup and buried the real
    # error under it.
    conn = None
    cur = None
    try:
        conn = get_db_connection()
        cur = conn.cursor(cursor_factory=RealDictCursor)

        # Idempotency: the transaction id is unique, so a retry is a read.
        cur.execute('SELECT * FROM tb_transfer WHERE transaction_id = %s', (transaction_id,))
        existing = cur.fetchone()
        if existing:
            log_audit_event(transaction_id, 'IDEMPOTENT_RETRY', {'previous_status': existing['status']})
            return jsonify({
                "data": {
                    "transferMoney": {
                        "transactionId": transaction_id,
                        "status": existing['status'],
                        "message": "Transfer already processed"
                    }
                }
            })

        cur.execute('SELECT * FROM tb_account WHERE id = %s FOR UPDATE', (from_account_id,))
        from_account = cur.fetchone()

        if not from_account:
            log_audit_event(transaction_id, 'TRANSFER_FAILED', {'reason': 'From account not found'})
            return jsonify({"errors": [{"message": f"Account {from_account_id} not found"}]}), 404

        if from_account['status'] != 'active':
            log_audit_event(transaction_id, 'TRANSFER_FAILED', {'reason': f'From account {from_account["status"]}'})
            return jsonify({"errors": [{"message": f"Account is {from_account['status']}"}]}), 400

        cur.execute('SELECT * FROM tb_account WHERE id = %s FOR UPDATE', (to_account_id,))
        to_account = cur.fetchone()

        if not to_account:
            log_audit_event(transaction_id, 'TRANSFER_FAILED', {'reason': 'To account not found'})
            return jsonify({"errors": [{"message": f"Account {to_account_id} not found"}]}), 404

        if to_account['status'] != 'active':
            log_audit_event(transaction_id, 'TRANSFER_FAILED', {'reason': f'To account {to_account["status"]}'})
            return jsonify({"errors": [{"message": f"Receiver account is {to_account['status']}"}]}), 400

        if from_account['balance'] < amount:
            log_audit_event(transaction_id, 'TRANSFER_FAILED', {'reason': 'Insufficient funds'})
            return jsonify({"errors": [{"message": "Insufficient funds"}]}), 400

        cur.execute('UPDATE tb_account SET balance = balance - %s WHERE id = %s',
                    (amount, from_account_id))
        cur.execute('UPDATE tb_account SET balance = balance + %s WHERE id = %s',
                    (amount, to_account_id))

        # The ledger references accounts by their surrogate key, so the ids the
        # caller sent are resolved to pk_account here rather than stored raw.
        cur.execute(
            '''INSERT INTO tb_transfer (transaction_id, fk_from_account, fk_to_account, amount, status)
               SELECT %s, f.pk_account, t.pk_account, %s, %s
                 FROM tb_account f, tb_account t
                WHERE f.id = %s AND t.id = %s''',
            (transaction_id, amount, 'completed', from_account_id, to_account_id)
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
        if cur is not None:
            cur.close()
        if conn is not None:
            conn.close()


def handle_compensate_transfer(query, variables):
    """Manual compensation - return funds if the transfer failed downstream."""
    transaction_id = arg(query, variables, 'transactionId')

    logger.info(f"Compensating transfer: {transaction_id}")

    conn = None
    cur = None
    try:
        conn = get_db_connection()
        cur = conn.cursor(cursor_factory=RealDictCursor)

        # Resolve the surrogate keys back to the account ids the caller knows.
        cur.execute(
            '''SELECT t.transaction_id, t.amount, t.status,
                      f.id AS from_account_id, o.id AS to_account_id
                 FROM tb_transfer t
                 JOIN tb_account f ON f.pk_account = t.fk_from_account
                 LEFT JOIN tb_account o ON o.pk_account = t.fk_to_account
                WHERE t.transaction_id = %s''',
            (transaction_id,)
        )
        transfer = cur.fetchone()

        if not transfer:
            return jsonify({"errors": [{"message": f"Transfer {transaction_id} not found"}]}), 404

        cur.execute('SELECT * FROM tb_compensation_record WHERE transaction_id = %s',
                    (transaction_id,))
        if cur.fetchone():
            return jsonify({
                "data": {
                    "compensateTransfer": {
                        "transactionId": transaction_id,
                        "status": "already_compensated"
                    }
                }
            })

        cur.execute('UPDATE tb_account SET balance = balance - %s WHERE id = %s',
                    (transfer['amount'], transfer['to_account_id']))
        cur.execute('UPDATE tb_account SET balance = balance + %s WHERE id = %s',
                    (transfer['amount'], transfer['from_account_id']))

        cur.execute(
            '''INSERT INTO tb_compensation_record (transaction_id, compensation_type, status)
               VALUES (%s, %s, %s)''',
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
        if cur is not None:
            cur.close()
        if conn is not None:
            conn.close()


def handle_get_account(query, variables):
    """Get account balance"""
    account_id = arg(query, variables, 'accountId')

    conn = None
    cur = None
    try:
        conn = get_db_connection()
        cur = conn.cursor(cursor_factory=RealDictCursor)
        cur.execute('SELECT * FROM tb_account WHERE id = %s', (account_id,))
        account = cur.fetchone()

        if not account:
            return jsonify({"errors": [{"message": f"Account {account_id} not found"}]}), 404

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
    finally:
        if cur is not None:
            cur.close()
        if conn is not None:
            conn.close()


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
