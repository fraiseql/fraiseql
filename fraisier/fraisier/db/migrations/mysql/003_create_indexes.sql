-- MySQL Migration: Create indexes for optimal performance

-- Fraise state indexes
CREATE INDEX idx_fraise_state_name_env ON tb_fraise_state(fraise_name, environment_name);
CREATE INDEX idx_fraise_state_identifier ON tb_fraise_state(identifier);
CREATE INDEX idx_fraise_state_id ON tb_fraise_state(id);

-- Deployment indexes
CREATE INDEX idx_deployment_fraise_state_fk ON tb_deployment(fk_fraise_state);
CREATE INDEX idx_deployment_started_at ON tb_deployment(started_at DESC);
CREATE INDEX idx_deployment_identifier ON tb_deployment(identifier);
CREATE INDEX idx_deployment_id ON tb_deployment(id);
CREATE INDEX idx_deployment_status ON tb_deployment(status);

-- Webhook indexes
CREATE INDEX idx_webhook_event_deployment_fk ON tb_webhook_event(fk_deployment);
CREATE INDEX idx_webhook_event_received_at ON tb_webhook_event(received_at DESC);
CREATE INDEX idx_webhook_event_identifier ON tb_webhook_event(identifier);
CREATE INDEX idx_webhook_event_id ON tb_webhook_event(id);
CREATE INDEX idx_webhook_event_processed ON tb_webhook_event(processed);

-- Deployment lock indexes
CREATE INDEX idx_deployment_lock_service_provider ON tb_deployment_lock(service_name, provider_name);
CREATE INDEX idx_deployment_lock_expires_at ON tb_deployment_lock(expires_at);
