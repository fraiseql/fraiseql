-- PostgreSQL Migration: Create core deployment tables with trinity pattern
-- Uses BIGINT GENERATED ALWAYS AS IDENTITY for primary keys (production-grade)

CREATE TABLE IF NOT EXISTS tb_fraise_state (
    id UUID NOT NULL UNIQUE,
    identifier TEXT NOT NULL UNIQUE,
    pk_fraise_state BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,

    fraise_name TEXT NOT NULL,
    environment_name TEXT NOT NULL,
    job_name TEXT,
    current_version TEXT,
    last_deployed_at TIMESTAMPTZ,
    last_deployed_by TEXT,
    status TEXT DEFAULT 'unknown',

    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,

    UNIQUE(fraise_name, environment_name, job_name)
);

-- Deployment history table
CREATE TABLE IF NOT EXISTS tb_deployment (
    id UUID NOT NULL UNIQUE,
    identifier TEXT NOT NULL UNIQUE,
    pk_deployment BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,

    fk_fraise_state BIGINT NOT NULL REFERENCES tb_fraise_state(pk_fraise_state) ON DELETE CASCADE,

    fraise_name TEXT NOT NULL,
    environment_name TEXT NOT NULL,
    job_name TEXT,
    started_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ,
    duration_seconds FLOAT8,
    old_version TEXT,
    new_version TEXT,
    status TEXT NOT NULL,
    triggered_by TEXT,
    triggered_by_user TEXT,
    git_commit TEXT,
    git_branch TEXT,
    error_message TEXT,
    details JSONB,

    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

-- Webhook events table
CREATE TABLE IF NOT EXISTS tb_webhook_event (
    id UUID NOT NULL UNIQUE,
    identifier TEXT NOT NULL UNIQUE,
    pk_webhook_event BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,

    fk_deployment BIGINT REFERENCES tb_deployment(pk_deployment) ON DELETE SET NULL,

    received_at TIMESTAMPTZ NOT NULL,
    event_type TEXT NOT NULL,
    git_provider TEXT NOT NULL,
    branch_name TEXT,
    commit_sha TEXT,
    sender TEXT,
    payload JSONB,
    processed BOOLEAN DEFAULT false,

    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

-- Deployment locks table
CREATE TABLE IF NOT EXISTS tb_deployment_lock (
    pk_deployment_lock BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,

    service_name TEXT NOT NULL,
    provider_name TEXT NOT NULL,
    locked_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,

    UNIQUE(service_name, provider_name)
);
