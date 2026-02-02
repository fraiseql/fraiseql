-- Banking System Tables (Trinity Pattern)
-- Pattern: tb_* (table), pk_* (INTEGER primary key), id (federation ID), v_* (view)

DROP TABLE IF EXISTS tb_compensation_record CASCADE;
DROP TABLE IF EXISTS tb_audit_log CASCADE;
DROP TABLE IF EXISTS tb_transfer CASCADE;
DROP TABLE IF EXISTS tb_account CASCADE;

CREATE TABLE tb_account (
    pk_account SERIAL PRIMARY KEY,
    id VARCHAR(36) UNIQUE NOT NULL,
    account_number VARCHAR(50) UNIQUE NOT NULL,
    account_holder VARCHAR(255) NOT NULL,
    balance DECIMAL(15, 2) NOT NULL,
    status VARCHAR(50) NOT NULL DEFAULT 'active',
    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
);

-- Transfer ledger (for audit trail and idempotency)
CREATE TABLE tb_transfer (
    pk_transfer SERIAL PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,
    transaction_id VARCHAR(255) UNIQUE NOT NULL,
    fk_from_account INTEGER NOT NULL REFERENCES tb_account(pk_account),
    fk_to_account INTEGER REFERENCES tb_account(pk_account),
    amount DECIMAL(15, 2) NOT NULL,
    status VARCHAR(50) NOT NULL DEFAULT 'pending',
    description TEXT,
    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
);

-- Audit log (for compliance)
CREATE TABLE tb_audit_log (
    pk_log_entry SERIAL PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,
    transaction_id VARCHAR(255),
    event_type VARCHAR(100) NOT NULL,
    details JSONB,
    timestamp TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
);

-- Compensation records
CREATE TABLE tb_compensation_record (
    pk_compensation SERIAL PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,
    transaction_id VARCHAR(255),
    compensation_type VARCHAR(100) NOT NULL,
    original_step INT,
    status VARCHAR(50) DEFAULT 'pending',
    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
);

-- Indexes
CREATE INDEX idx_tb_account_id ON tb_account(id);
CREATE INDEX idx_tb_account_status ON tb_account(status);
CREATE INDEX idx_tb_transfer_id ON tb_transfer(id);
CREATE INDEX idx_tb_transfer_transaction_id ON tb_transfer(transaction_id);
CREATE INDEX idx_tb_transfer_fk_from_account ON tb_transfer(fk_from_account);
CREATE INDEX idx_tb_transfer_fk_to_account ON tb_transfer(fk_to_account);
CREATE INDEX idx_tb_transfer_status ON tb_transfer(status);
CREATE INDEX idx_tb_audit_log_id ON tb_audit_log(id);
CREATE INDEX idx_tb_audit_log_transaction_id ON tb_audit_log(transaction_id);
CREATE INDEX idx_tb_audit_log_event_type ON tb_audit_log(event_type);
CREATE INDEX idx_tb_compensation_record_id ON tb_compensation_record(id);
CREATE INDEX idx_tb_compensation_record_transaction_id ON tb_compensation_record(transaction_id);

-- Create views (Trinity Pattern v_* naming)
-- Returns pk_* (for internal joins) and data (JSONB for GraphQL)
CREATE VIEW v_account AS
SELECT
    pk_account,
    jsonb_build_object(
        'id', id,
        'accountNumber', account_number,
        'accountHolder', account_holder,
        'balance', balance,
        'status', status,
        'createdAt', created_at,
        'updatedAt', updated_at
    ) AS data
FROM tb_account;

CREATE VIEW v_transfer AS
SELECT
    pk_transfer,
    jsonb_build_object(
        'id', id,
        'transactionId', transaction_id,
        'amount', amount,
        'status', status,
        'description', description,
        'createdAt', created_at,
        'updatedAt', updated_at
    ) AS data
FROM tb_transfer;

CREATE VIEW v_audit_log AS
SELECT
    pk_log_entry,
    jsonb_build_object(
        'id', id,
        'transactionId', transaction_id,
        'eventType', event_type,
        'details', details,
        'timestamp', timestamp
    ) AS data
FROM tb_audit_log;

CREATE VIEW v_compensation_record AS
SELECT
    pk_compensation,
    jsonb_build_object(
        'id', id,
        'transactionId', transaction_id,
        'compensationType', compensation_type,
        'originalStep', original_step,
        'status', status,
        'createdAt', created_at
    ) AS data
FROM tb_compensation_record;

-- Sample accounts
INSERT INTO tb_account (id, account_number, account_holder, balance, status) VALUES
  ('acc-001', 'CHK-001', 'Alice Johnson', 1000.00, 'active'),
  ('acc-002', 'SAV-001', 'Bob Smith', 500.00, 'active'),
  ('acc-003', 'BUS-001', 'Carol White', 5000.00, 'active'),
  ('acc-004', 'CHK-002', 'David Brown', 100.00, 'frozen')
ON CONFLICT (id) DO NOTHING;
