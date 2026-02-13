-- SQLite Migration: Create indexes for optimal query performance

-- Fraise state lookups
CREATE INDEX IF NOT EXISTS idx_fraise_state_name_env
    ON tb_fraise_state(fraise_name, environment_name);

CREATE INDEX IF NOT EXISTS idx_fraise_state_identifier
    ON tb_fraise_state(identifier);

CREATE INDEX IF NOT EXISTS idx_fraise_state_id
    ON tb_fraise_state(id);

-- Deployment lookups
CREATE INDEX IF NOT EXISTS idx_deployment_fraise_state_fk
    ON tb_deployment(fk_fraise_state);

CREATE INDEX IF NOT EXISTS idx_deployment_started_at
    ON tb_deployment(started_at DESC);

CREATE INDEX IF NOT EXISTS idx_deployment_identifier
    ON tb_deployment(identifier);

CREATE INDEX IF NOT EXISTS idx_deployment_id
    ON tb_deployment(id);

CREATE INDEX IF NOT EXISTS idx_deployment_status
    ON tb_deployment(status);

-- Webhook lookups
CREATE INDEX IF NOT EXISTS idx_webhook_event_deployment_fk
    ON tb_webhook_event(fk_deployment);

CREATE INDEX IF NOT EXISTS idx_webhook_event_received_at
    ON tb_webhook_event(received_at DESC);

CREATE INDEX IF NOT EXISTS idx_webhook_event_identifier
    ON tb_webhook_event(identifier);

CREATE INDEX IF NOT EXISTS idx_webhook_event_id
    ON tb_webhook_event(id);

CREATE INDEX IF NOT EXISTS idx_webhook_event_processed
    ON tb_webhook_event(processed);

-- Deployment lock lookups
CREATE INDEX IF NOT EXISTS idx_deployment_lock_service_provider
    ON tb_deployment_lock(service_name, provider_name);

CREATE INDEX IF NOT EXISTS idx_deployment_lock_expires_at
    ON tb_deployment_lock(expires_at);
