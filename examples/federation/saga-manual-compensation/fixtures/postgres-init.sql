-- Banking System Tables (Trinity Pattern)
-- Pattern: tb_* (table), pk_* (INTEGER primary key), id (federation ID), v_* (view)

DROP TABLE IF EXISTS tb_compensation_records CASCADE;
DROP TABLE IF EXISTS tb_audit_log CASCADE;
DROP TABLE IF EXISTS tb_transfers CASCADE;
DROP TABLE IF EXISTS tb_accounts CASCADE;

CREATE TABLE tb_accounts (
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
CREATE TABLE tb_transfers (
    pk_transfer SERIAL PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,
    transaction_id VARCHAR(255) UNIQUE NOT NULL,
    fk_from_account INTEGER NOT NULL REFERENCES tb_accounts(pk_account),
    fk_to_account INTEGER REFERENCES tb_accounts(pk_account),
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
CREATE TABLE tb_compensation_records (
    pk_compensation SERIAL PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,
    transaction_id VARCHAR(255),
    compensation_type VARCHAR(100) NOT NULL,
    original_step INT,
    status VARCHAR(50) DEFAULT 'pending',
    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
);

-- Indexes
CREATE INDEX idx_tb_accounts_id ON tb_accounts(id);
CREATE INDEX idx_tb_accounts_status ON tb_accounts(status);
CREATE INDEX idx_tb_transfers_id ON tb_transfers(id);
CREATE INDEX idx_tb_transfers_transaction_id ON tb_transfers(transaction_id);
CREATE INDEX idx_tb_transfers_fk_from_account ON tb_transfers(fk_from_account);
CREATE INDEX idx_tb_transfers_fk_to_account ON tb_transfers(fk_to_account);
CREATE INDEX idx_tb_transfers_status ON tb_transfers(status);
CREATE INDEX idx_tb_audit_log_id ON tb_audit_log(id);
CREATE INDEX idx_tb_audit_log_transaction_id ON tb_audit_log(transaction_id);
CREATE INDEX idx_tb_audit_log_event_type ON tb_audit_log(event_type);
CREATE INDEX idx_tb_compensation_id ON tb_compensation_records(id);
CREATE INDEX idx_tb_compensation_transaction_id ON tb_compensation_records(transaction_id);

-- Create views (Trinity Pattern v_* naming)
CREATE VIEW v_accounts AS
SELECT pk_account, id, account_number, account_holder, balance, status, created_at, updated_at
FROM tb_accounts;

CREATE VIEW v_transfers AS
SELECT pk_transfer, id, transaction_id, fk_from_account, fk_to_account, amount, status, description, created_at, updated_at
FROM tb_transfers;

CREATE VIEW v_audit_log AS
SELECT pk_log_entry, id, transaction_id, event_type, details, timestamp
FROM tb_audit_log;

CREATE VIEW v_compensation_records AS
SELECT pk_compensation, id, transaction_id, compensation_type, original_step, status, created_at
FROM tb_compensation_records;

-- Sample accounts
INSERT INTO tb_accounts (id, account_number, account_holder, balance, status) VALUES
  ('acc-001', 'CHK-001', 'Alice Johnson', 1000.00, 'active'),
  ('acc-002', 'SAV-001', 'Bob Smith', 500.00, 'active'),
  ('acc-003', 'BUS-001', 'Carol White', 5000.00, 'active'),
  ('acc-004', 'CHK-002', 'David Brown', 100.00, 'frozen')
ON CONFLICT (id) DO NOTHING;
