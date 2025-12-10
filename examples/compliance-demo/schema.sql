-- Compliance Demo Schema
-- =====================================================================
-- Demonstrates:
-- - Cryptographic audit trails with HMAC chains
-- - Immutable audit logs
-- - Data integrity verification
-- - SLSA provenance tracking
-- - KMS integration preparation
-- =====================================================================

-- Enable required extensions
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pgcrypto";

-- ============================================================================
-- BASE TABLES (tb_*)
-- ============================================================================

-- Documents (example sensitive data)
CREATE TABLE tb_document (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    title TEXT NOT NULL,
    content TEXT NOT NULL,
    classification TEXT NOT NULL DEFAULT 'public', -- public, confidential, secret
    created_by TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    version INT NOT NULL DEFAULT 1,
    checksum TEXT NOT NULL -- SHA-256 hash for integrity
);

-- Cryptographic Audit Trail (immutable)
CREATE TABLE tb_audit_trail (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    sequence_number BIGSERIAL UNIQUE NOT NULL,
    event_type TEXT NOT NULL, -- document_created, document_updated, document_accessed, etc.
    resource_type TEXT NOT NULL,
    resource_id UUID NOT NULL,
    actor TEXT NOT NULL, -- user email or service account
    action_details JSONB NOT NULL,
    ip_address INET,
    user_agent TEXT,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Cryptographic chain
    previous_hash TEXT, -- Hash of previous audit entry
    current_hash TEXT NOT NULL, -- Hash of this entry
    hmac_signature TEXT NOT NULL, -- HMAC signature for integrity

    -- Compliance metadata
    compliance_markers JSONB DEFAULT '{}', -- GDPR, HIPAA, SOC2, etc.
    retention_until TIMESTAMPTZ -- For compliance retention policies
);

-- SLSA Provenance Tracking
CREATE TABLE tb_slsa_provenance (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    artifact_name TEXT NOT NULL,
    artifact_version TEXT NOT NULL,
    artifact_digest TEXT NOT NULL, -- SHA-256 digest
    build_type TEXT NOT NULL, -- github-actions, gitlab-ci, etc.
    builder_id TEXT NOT NULL, -- URI of the builder
    build_invocation_id TEXT NOT NULL,
    build_started_on TIMESTAMPTZ NOT NULL,
    build_finished_on TIMESTAMPTZ NOT NULL,
    source_repo TEXT NOT NULL,
    source_commit TEXT NOT NULL,
    source_commit_digest TEXT NOT NULL,
    materials JSONB NOT NULL, -- Dependencies and their digests
    metadata JSONB NOT NULL, -- Additional SLSA metadata
    attestation TEXT NOT NULL, -- Full SLSA attestation (base64 encoded)
    verified BOOLEAN NOT NULL DEFAULT false,
    verified_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- KMS Key Management
CREATE TABLE tb_kms_key (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    key_id TEXT NOT NULL UNIQUE, -- External KMS key ID (AWS KMS, GCP KMS, Vault)
    key_provider TEXT NOT NULL, -- aws-kms, gcp-kms, hashicorp-vault
    key_purpose TEXT NOT NULL, -- audit-signing, data-encryption, document-signing
    key_algorithm TEXT NOT NULL, -- HMAC-SHA256, AES-256-GCM, RSA-2048
    key_status TEXT NOT NULL DEFAULT 'active', -- active, rotated, revoked
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    rotated_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ,
    metadata JSONB DEFAULT '{}'
);

-- Data Encryption Registry
CREATE TABLE tb_encrypted_field (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    table_name TEXT NOT NULL,
    field_name TEXT NOT NULL,
    record_id UUID NOT NULL,
    kms_key_id UUID NOT NULL REFERENCES tb_kms_key(id),
    encryption_algorithm TEXT NOT NULL,
    encrypted_value BYTEA NOT NULL, -- Encrypted data
    encryption_context JSONB, -- Additional context for decryption
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(table_name, field_name, record_id)
);

-- ============================================================================
-- INDEXES
-- ============================================================================

CREATE INDEX idx_document_classification ON tb_document(classification);
CREATE INDEX idx_document_created_by ON tb_document(created_by);

CREATE INDEX idx_audit_sequence ON tb_audit_trail(sequence_number);
CREATE INDEX idx_audit_resource ON tb_audit_trail(resource_type, resource_id);
CREATE INDEX idx_audit_actor ON tb_audit_trail(actor);
CREATE INDEX idx_audit_timestamp ON tb_audit_trail(timestamp DESC);
CREATE INDEX idx_audit_event_type ON tb_audit_trail(event_type);

CREATE INDEX idx_slsa_artifact ON tb_slsa_provenance(artifact_name, artifact_version);
CREATE INDEX idx_slsa_commit ON tb_slsa_provenance(source_commit);
CREATE INDEX idx_slsa_verified ON tb_slsa_provenance(verified);

CREATE INDEX idx_kms_provider ON tb_kms_key(key_provider, key_status);
CREATE INDEX idx_encrypted_field_record ON tb_encrypted_field(table_name, record_id);

-- ============================================================================
-- VIEWS (v_*)
-- ============================================================================

CREATE VIEW v_document AS
SELECT
    id,
    title,
    content,
    classification,
    created_by,
    created_at,
    updated_at,
    version,
    checksum
FROM tb_document;

CREATE VIEW v_audit_trail AS
SELECT
    id,
    sequence_number,
    event_type,
    resource_type,
    resource_id,
    actor,
    action_details,
    ip_address,
    user_agent,
    timestamp,
    previous_hash,
    current_hash,
    hmac_signature,
    compliance_markers,
    retention_until
FROM tb_audit_trail;

CREATE VIEW v_slsa_provenance AS
SELECT
    id,
    artifact_name,
    artifact_version,
    artifact_digest,
    build_type,
    builder_id,
    source_repo,
    source_commit,
    verified,
    verified_at,
    created_at
FROM tb_slsa_provenance;

CREATE VIEW v_kms_key AS
SELECT
    id,
    key_id,
    key_provider,
    key_purpose,
    key_algorithm,
    key_status,
    created_at,
    rotated_at
FROM tb_kms_key;

-- ============================================================================
-- COMPUTED VIEWS (tv_*)
-- ============================================================================

-- Document with audit history
CREATE VIEW tv_document AS
SELECT
    d.id,
    d.title,
    d.content,
    d.classification,
    d.created_by,
    d.created_at,
    d.updated_at,
    d.version,
    d.checksum,
    (
        SELECT COUNT(*)
        FROM tb_audit_trail
        WHERE resource_type = 'document' AND resource_id = d.id
    ) as audit_count,
    (
        SELECT MAX(timestamp)
        FROM tb_audit_trail
        WHERE resource_type = 'document' AND resource_id = d.id
    ) as last_audit_at
FROM tb_document d;

-- Audit trail with chain verification status
CREATE VIEW tv_audit_trail AS
SELECT
    a.id,
    a.sequence_number,
    a.event_type,
    a.resource_type,
    a.resource_id,
    a.actor,
    a.action_details,
    a.timestamp,
    a.current_hash,
    a.hmac_signature,
    a.compliance_markers,
    -- Verify chain integrity
    CASE
        WHEN a.sequence_number = 1 THEN true
        WHEN a.previous_hash = (
            SELECT current_hash
            FROM tb_audit_trail
            WHERE sequence_number = a.sequence_number - 1
        ) THEN true
        ELSE false
    END as chain_valid
FROM tb_audit_trail a;

-- ============================================================================
-- FUNCTIONS (fn_*)
-- ============================================================================

-- Create document with automatic audit trail
CREATE OR REPLACE FUNCTION fn_create_document(
    input_title TEXT,
    input_content TEXT,
    input_classification TEXT,
    input_created_by TEXT
) RETURNS UUID AS $$
DECLARE
    new_doc_id UUID;
    doc_checksum TEXT;
BEGIN
    -- Calculate checksum
    doc_checksum := encode(digest(input_content, 'sha256'), 'hex');

    -- Create document
    INSERT INTO tb_document (title, content, classification, created_by, checksum)
    VALUES (input_title, input_content, input_classification, input_created_by, doc_checksum)
    RETURNING id INTO new_doc_id;

    -- Create audit entry
    PERFORM fn_create_audit_entry(
        'document_created',
        'document',
        new_doc_id,
        input_created_by,
        jsonb_build_object(
            'title', input_title,
            'classification', input_classification,
            'checksum', doc_checksum
        )
    );

    RETURN new_doc_id;
END;
$$ LANGUAGE plpgsql VOLATILE;

-- Create cryptographic audit entry
CREATE OR REPLACE FUNCTION fn_create_audit_entry(
    input_event_type TEXT,
    input_resource_type TEXT,
    input_resource_id UUID,
    input_actor TEXT,
    input_action_details JSONB
) RETURNS UUID AS $$
DECLARE
    new_audit_id UUID;
    prev_hash TEXT;
    current_hash TEXT;
    hmac_sig TEXT;
    hmac_key TEXT := 'compliance-demo-secret-key'; -- In production, use KMS
BEGIN
    -- Get previous hash
    SELECT a.current_hash INTO prev_hash
    FROM tb_audit_trail a
    ORDER BY sequence_number DESC
    LIMIT 1;

    -- Calculate current hash
    current_hash := encode(
        digest(
            input_event_type ||
            input_resource_type ||
            input_resource_id::TEXT ||
            input_actor ||
            input_action_details::TEXT ||
            COALESCE(prev_hash, ''),
            'sha256'
        ),
        'hex'
    );

    -- Calculate HMAC signature
    hmac_sig := encode(
        hmac(current_hash, hmac_key, 'sha256'),
        'hex'
    );

    -- Insert audit entry
    INSERT INTO tb_audit_trail (
        event_type,
        resource_type,
        resource_id,
        actor,
        action_details,
        previous_hash,
        current_hash,
        hmac_signature
    ) VALUES (
        input_event_type,
        input_resource_type,
        input_resource_id,
        input_actor,
        input_action_details,
        prev_hash,
        current_hash,
        hmac_sig
    ) RETURNING id INTO new_audit_id;

    RETURN new_audit_id;
END;
$$ LANGUAGE plpgsql VOLATILE;

-- Verify audit chain integrity
CREATE OR REPLACE FUNCTION fn_verify_audit_chain()
RETURNS TABLE(
    sequence_number BIGINT,
    valid BOOLEAN,
    error_message TEXT
) AS $$
BEGIN
    RETURN QUERY
    SELECT
        a.sequence_number,
        CASE
            WHEN a.sequence_number = 1 THEN true
            WHEN a.previous_hash = prev.current_hash THEN true
            ELSE false
        END as valid,
        CASE
            WHEN a.sequence_number = 1 THEN 'First entry (genesis)'
            WHEN a.previous_hash = prev.current_hash THEN 'Valid chain link'
            ELSE 'Chain broken: hash mismatch'
        END as error_message
    FROM tb_audit_trail a
    LEFT JOIN tb_audit_trail prev ON prev.sequence_number = a.sequence_number - 1
    ORDER BY a.sequence_number;
END;
$$ LANGUAGE plpgsql STABLE;

-- Record SLSA provenance
CREATE OR REPLACE FUNCTION fn_record_slsa_provenance(
    input_artifact_name TEXT,
    input_artifact_version TEXT,
    input_artifact_digest TEXT,
    input_build_type TEXT,
    input_source_commit TEXT,
    input_attestation TEXT
) RETURNS UUID AS $$
DECLARE
    new_provenance_id UUID;
BEGIN
    INSERT INTO tb_slsa_provenance (
        artifact_name,
        artifact_version,
        artifact_digest,
        build_type,
        builder_id,
        build_invocation_id,
        build_started_on,
        build_finished_on,
        source_repo,
        source_commit,
        source_commit_digest,
        materials,
        metadata,
        attestation
    ) VALUES (
        input_artifact_name,
        input_artifact_version,
        input_artifact_digest,
        input_build_type,
        'https://github.com/fraiseql/fraiseql/.github/workflows/publish.yml',
        gen_random_uuid()::TEXT,
        NOW() - INTERVAL '10 minutes',
        NOW(),
        'https://github.com/fraiseql/fraiseql',
        input_source_commit,
        encode(digest(input_source_commit, 'sha256'), 'hex'),
        '[]'::JSONB,
        '{}'::JSONB,
        input_attestation
    ) RETURNING id INTO new_provenance_id;

    RETURN new_provenance_id;
END;
$$ LANGUAGE plpgsql VOLATILE;

-- ============================================================================
-- SEED DATA
-- ============================================================================

-- Sample KMS keys
INSERT INTO tb_kms_key (key_id, key_provider, key_purpose, key_algorithm, key_status)
VALUES
    ('arn:aws:kms:us-east-1:123456789012:key/audit-hmac', 'aws-kms', 'audit-signing', 'HMAC-SHA256', 'active'),
    ('projects/myproject/locations/global/keyRings/compliance/cryptoKeys/data-encryption', 'gcp-kms', 'data-encryption', 'AES-256-GCM', 'active'),
    ('vault/fraiseql/compliance/signing-key', 'hashicorp-vault', 'document-signing', 'RSA-2048', 'active');

-- Sample documents
INSERT INTO tb_document (id, title, content, classification, created_by, checksum)
VALUES
    (
        '11111111-1111-1111-1111-111111111111',
        'Security Policy v1.0',
        'This document outlines our security practices...',
        'confidential',
        'admin@example.com',
        encode(digest('This document outlines our security practices...', 'sha256'), 'hex')
    ),
    (
        '22222222-2222-2222-2222-222222222222',
        'Public API Documentation',
        'Our GraphQL API provides the following endpoints...',
        'public',
        'tech-writer@example.com',
        encode(digest('Our GraphQL API provides the following endpoints...', 'sha256'), 'hex')
    );

-- Sample audit entries (with proper chain)
DO $$
DECLARE
    prev_hash TEXT := NULL;
    current_hash TEXT;
    hmac_sig TEXT;
    hmac_key TEXT := 'compliance-demo-secret-key';
BEGIN
    -- Entry 1
    current_hash := encode(
        digest('document_created' || 'document' || '11111111-1111-1111-1111-111111111111' || 'admin@example.com' || '{}', 'sha256'),
        'hex'
    );
    hmac_sig := encode(hmac(current_hash, hmac_key, 'sha256'), 'hex');

    INSERT INTO tb_audit_trail (event_type, resource_type, resource_id, actor, action_details, previous_hash, current_hash, hmac_signature)
    VALUES ('document_created', 'document', '11111111-1111-1111-1111-111111111111', 'admin@example.com', '{"action":"created"}'::JSONB, NULL, current_hash, hmac_sig);

    prev_hash := current_hash;

    -- Entry 2
    current_hash := encode(
        digest('document_accessed' || 'document' || '11111111-1111-1111-1111-111111111111' || 'user@example.com' || '{}' || prev_hash, 'sha256'),
        'hex'
    );
    hmac_sig := encode(hmac(current_hash, hmac_key, 'sha256'), 'hex');

    INSERT INTO tb_audit_trail (event_type, resource_type, resource_id, actor, action_details, previous_hash, current_hash, hmac_signature)
    VALUES ('document_accessed', 'document', '11111111-1111-1111-1111-111111111111', 'user@example.com', '{"action":"read"}'::JSONB, prev_hash, current_hash, hmac_sig);
END $$;

-- Sample SLSA provenance
INSERT INTO tb_slsa_provenance (
    artifact_name,
    artifact_version,
    artifact_digest,
    build_type,
    builder_id,
    build_invocation_id,
    build_started_on,
    build_finished_on,
    source_repo,
    source_commit,
    source_commit_digest,
    materials,
    metadata,
    attestation,
    verified
) VALUES (
    'fraiseql',
    '0.1.0',
    'sha256:abc123def456...',
    'github-actions',
    'https://github.com/fraiseql/fraiseql/.github/workflows/publish.yml@refs/heads/main',
    'run-123456789',
    NOW() - INTERVAL '1 hour',
    NOW() - INTERVAL '50 minutes',
    'https://github.com/fraiseql/fraiseql',
    'abc123def456',
    encode(digest('abc123def456', 'sha256'), 'hex'),
    '[{"uri": "pkg:pypi/fastapi@0.104.0", "digest": {"sha256": "..."}}]'::JSONB,
    '{"slsaVersion": "1.0", "buildConfig": {}}'::JSONB,
    'base64-encoded-attestation-here',
    true
);

-- ============================================================================
-- COMMENTS
-- ============================================================================

COMMENT ON TABLE tb_audit_trail IS 'Immutable cryptographic audit trail with HMAC chain';
COMMENT ON TABLE tb_slsa_provenance IS 'SLSA provenance attestations for supply chain security';
COMMENT ON TABLE tb_kms_key IS 'Key Management System integration for cryptographic operations';
COMMENT ON TABLE tb_encrypted_field IS 'Registry of encrypted fields with KMS key references';

COMMENT ON FUNCTION fn_create_audit_entry IS 'Creates cryptographically signed audit entry in chain';
COMMENT ON FUNCTION fn_verify_audit_chain IS 'Verifies integrity of audit trail chain';
