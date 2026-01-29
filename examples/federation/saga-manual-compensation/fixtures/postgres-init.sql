-- Banking System Tables
CREATE TABLE accounts (
    id VARCHAR(36) PRIMARY KEY,
    account_number VARCHAR(50) UNIQUE NOT NULL,
    account_holder VARCHAR(255) NOT NULL,
    balance DECIMAL(15, 2) NOT NULL,
    status VARCHAR(50) NOT NULL DEFAULT 'active',
    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
);

-- Transfer ledger (for audit trail and idempotency)
CREATE TABLE transfers (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    transaction_id VARCHAR(255) UNIQUE NOT NULL,
    from_account_id VARCHAR(36) NOT NULL REFERENCES accounts(id),
    to_account_id VARCHAR(36) REFERENCES accounts(id),
    amount DECIMAL(15, 2) NOT NULL,
    status VARCHAR(50) NOT NULL DEFAULT 'pending',
    description TEXT,
    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
);

-- Audit log (for compliance)
CREATE TABLE audit_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    transaction_id VARCHAR(255),
    event_type VARCHAR(100) NOT NULL,
    details JSONB,
    timestamp TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
);

-- Compensation records
CREATE TABLE compensation_records (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    transaction_id VARCHAR(255),
    compensation_type VARCHAR(100) NOT NULL,
    original_step INT,
    status VARCHAR(50) DEFAULT 'pending',
    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
);

-- Indexes
CREATE INDEX idx_accounts_status ON accounts(status);
CREATE INDEX idx_transfers_transaction_id ON transfers(transaction_id);
CREATE INDEX idx_transfers_from_account ON transfers(from_account_id);
CREATE INDEX idx_transfers_to_account ON transfers(to_account_id);
CREATE INDEX idx_transfers_status ON transfers(status);
CREATE INDEX idx_audit_log_transaction_id ON audit_log(transaction_id);
CREATE INDEX idx_audit_log_event_type ON audit_log(event_type);
CREATE INDEX idx_compensation_transaction_id ON compensation_records(transaction_id);

-- Sample accounts
INSERT INTO accounts (id, account_number, account_holder, balance, status) VALUES
  ('acc-001', 'CHK-001', 'Alice Johnson', 1000.00, 'active'),
  ('acc-002', 'SAV-001', 'Bob Smith', 500.00, 'active'),
  ('acc-003', 'BUS-001', 'Carol White', 5000.00, 'active'),
  ('acc-004', 'CHK-002', 'David Brown', 100.00, 'frozen')
ON CONFLICT DO NOTHING;
