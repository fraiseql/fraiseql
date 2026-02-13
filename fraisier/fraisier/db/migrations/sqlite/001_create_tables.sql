-- SQLite Migration: Create core deployment tables with trinity pattern
-- This migration creates the write-side tables following CQRS pattern

-- Fraise state table - tracks current deployed version and status per fraise/environment
CREATE TABLE IF NOT EXISTS tb_fraise_state (
    id TEXT NOT NULL UNIQUE,
    identifier TEXT NOT NULL UNIQUE,
    pk_fraise_state INTEGER PRIMARY KEY AUTOINCREMENT,

    fraise_name TEXT NOT NULL,
    environment_name TEXT NOT NULL,
    job_name TEXT,
    current_version TEXT,
    last_deployed_at TEXT,
    last_deployed_by TEXT,
    status TEXT DEFAULT 'unknown',

    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,

    UNIQUE(fraise_name, environment_name, job_name)
);

-- Deployment history table - complete record of all deployments
CREATE TABLE IF NOT EXISTS tb_deployment (
    id TEXT NOT NULL UNIQUE,
    identifier TEXT NOT NULL UNIQUE,
    pk_deployment INTEGER PRIMARY KEY AUTOINCREMENT,

    fk_fraise_state INTEGER NOT NULL REFERENCES tb_fraise_state(pk_fraise_state),

    fraise_name TEXT NOT NULL,
    environment_name TEXT NOT NULL,
    job_name TEXT,
    started_at TEXT NOT NULL,
    completed_at TEXT,
    duration_seconds REAL,
    old_version TEXT,
    new_version TEXT,
    status TEXT NOT NULL,
    triggered_by TEXT,
    triggered_by_user TEXT,
    git_commit TEXT,
    git_branch TEXT,
    error_message TEXT,
    details TEXT,

    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- Webhook events table - tracks received git webhooks
CREATE TABLE IF NOT EXISTS tb_webhook_event (
    id TEXT NOT NULL UNIQUE,
    identifier TEXT NOT NULL UNIQUE,
    pk_webhook_event INTEGER PRIMARY KEY AUTOINCREMENT,

    fk_deployment INTEGER REFERENCES tb_deployment(pk_deployment),

    received_at TEXT NOT NULL,
    event_type TEXT NOT NULL,
    git_provider TEXT NOT NULL,
    branch_name TEXT,
    commit_sha TEXT,
    sender TEXT,
    payload TEXT,
    processed INTEGER DEFAULT 0,

    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- Deployment locks table - prevents concurrent deployments to same service/provider
CREATE TABLE IF NOT EXISTS tb_deployment_lock (
    pk_deployment_lock INTEGER PRIMARY KEY AUTOINCREMENT,

    service_name TEXT NOT NULL,
    provider_name TEXT NOT NULL,
    locked_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,

    UNIQUE(service_name, provider_name)
);
