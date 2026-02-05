-- MySQL Migration: Create core deployment tables with trinity pattern
-- Uses AUTO_INCREMENT BIGINT for primary keys, VARCHAR for UUIDs

CREATE TABLE IF NOT EXISTS tb_fraise_state (
    id VARCHAR(36) NOT NULL UNIQUE,
    identifier VARCHAR(255) NOT NULL UNIQUE,
    pk_fraise_state BIGINT NOT NULL AUTO_INCREMENT PRIMARY KEY,

    fraise_name VARCHAR(255) NOT NULL,
    environment_name VARCHAR(255) NOT NULL,
    job_name VARCHAR(255),
    current_version VARCHAR(255),
    last_deployed_at DATETIME,
    last_deployed_by VARCHAR(255),
    status VARCHAR(50) DEFAULT 'unknown',

    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL,

    UNIQUE KEY uk_fraise_state_combo (fraise_name, environment_name, job_name)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

-- Deployment history table
CREATE TABLE IF NOT EXISTS tb_deployment (
    id VARCHAR(36) NOT NULL UNIQUE,
    identifier VARCHAR(255) NOT NULL UNIQUE,
    pk_deployment BIGINT NOT NULL AUTO_INCREMENT PRIMARY KEY,

    fk_fraise_state BIGINT NOT NULL REFERENCES tb_fraise_state(pk_fraise_state) ON DELETE CASCADE,

    fraise_name VARCHAR(255) NOT NULL,
    environment_name VARCHAR(255) NOT NULL,
    job_name VARCHAR(255),
    started_at DATETIME NOT NULL,
    completed_at DATETIME,
    duration_seconds DOUBLE,
    old_version VARCHAR(255),
    new_version VARCHAR(255),
    status VARCHAR(50) NOT NULL,
    triggered_by VARCHAR(50),
    triggered_by_user VARCHAR(255),
    git_commit VARCHAR(40),
    git_branch VARCHAR(255),
    error_message LONGTEXT,
    details JSON,

    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

-- Webhook events table
CREATE TABLE IF NOT EXISTS tb_webhook_event (
    id VARCHAR(36) NOT NULL UNIQUE,
    identifier VARCHAR(255) NOT NULL UNIQUE,
    pk_webhook_event BIGINT NOT NULL AUTO_INCREMENT PRIMARY KEY,

    fk_deployment BIGINT REFERENCES tb_deployment(pk_deployment) ON DELETE SET NULL,

    received_at DATETIME NOT NULL,
    event_type VARCHAR(50) NOT NULL,
    git_provider VARCHAR(50) NOT NULL,
    branch_name VARCHAR(255),
    commit_sha VARCHAR(40),
    sender VARCHAR(255),
    payload LONGTEXT,
    processed TINYINT DEFAULT 0,

    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

-- Deployment locks table
CREATE TABLE IF NOT EXISTS tb_deployment_lock (
    pk_deployment_lock BIGINT NOT NULL AUTO_INCREMENT PRIMARY KEY,

    service_name VARCHAR(255) NOT NULL,
    provider_name VARCHAR(255) NOT NULL,
    locked_at DATETIME NOT NULL,
    expires_at DATETIME NOT NULL,

    UNIQUE KEY uk_lock_service_provider (service_name, provider_name)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
