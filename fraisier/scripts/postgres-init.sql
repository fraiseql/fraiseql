-- PostgreSQL initialization script for Fraisier
-- Creates necessary schemas, extensions, and tables

-- Enable required extensions
CREATE EXTENSION IF NOT EXISTS uuid-ossp;
CREATE EXTENSION IF NOT EXISTS pg_trgm;
CREATE EXTENSION IF NOT EXISTS btree_gin;

-- Create schemas
CREATE SCHEMA IF NOT EXISTS fraisier;
GRANT USAGE ON SCHEMA fraisier TO fraisier;
ALTER DEFAULT PRIVILEGES IN SCHEMA fraisier GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO fraisier;

CREATE SCHEMA IF NOT EXISTS migrations;
GRANT USAGE ON SCHEMA migrations TO fraisier;

-- ============================================================================
-- Core Deployment Tables
-- ============================================================================

CREATE TABLE IF NOT EXISTS fraisier.deployments (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    service_name VARCHAR(255) NOT NULL,
    old_version VARCHAR(255) NOT NULL,
    new_version VARCHAR(255) NOT NULL,
    strategy VARCHAR(50) NOT NULL,
    status VARCHAR(50) NOT NULL DEFAULT 'pending',
    started_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    completed_at TIMESTAMP WITH TIME ZONE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    metadata JSONB DEFAULT '{}'::jsonb
);

CREATE INDEX IF NOT EXISTS idx_deployments_service_name ON fraisier.deployments(service_name);
CREATE INDEX IF NOT EXISTS idx_deployments_status ON fraisier.deployments(status);
CREATE INDEX IF NOT EXISTS idx_deployments_created_at ON fraisier.deployments(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_deployments_metadata ON fraisier.deployments USING GIN(metadata);

-- ============================================================================
-- Deployment Events Table (for audit trail)
-- ============================================================================

CREATE TABLE IF NOT EXISTS fraisier.deployment_events (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    deployment_id UUID NOT NULL REFERENCES fraisier.deployments(id) ON DELETE CASCADE,
    event_type VARCHAR(100) NOT NULL,
    status VARCHAR(50) NOT NULL,
    message TEXT,
    details JSONB DEFAULT '{}'::jsonb,
    timestamp TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_deployment_events_deployment_id ON fraisier.deployment_events(deployment_id);
CREATE INDEX IF NOT EXISTS idx_deployment_events_event_type ON fraisier.deployment_events(event_type);
CREATE INDEX IF NOT EXISTS idx_deployment_events_timestamp ON fraisier.deployment_events(timestamp DESC);

-- ============================================================================
-- Health Check Results Table
-- ============================================================================

CREATE TABLE IF NOT EXISTS fraisier.health_checks (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    deployment_id UUID NOT NULL REFERENCES fraisier.deployments(id) ON DELETE CASCADE,
    service_name VARCHAR(255) NOT NULL,
    health_url VARCHAR(1000) NOT NULL,
    status_code INTEGER,
    response_time_ms INTEGER,
    healthy BOOLEAN NOT NULL,
    error_message TEXT,
    checked_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_health_checks_deployment_id ON fraisier.health_checks(deployment_id);
CREATE INDEX IF NOT EXISTS idx_health_checks_service_name ON fraisier.health_checks(service_name);
CREATE INDEX IF NOT EXISTS idx_health_checks_checked_at ON fraisier.health_checks(checked_at DESC);

-- ============================================================================
-- Metrics Table (for storing metrics during deployments)
-- ============================================================================

CREATE TABLE IF NOT EXISTS fraisier.deployment_metrics (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    deployment_id UUID NOT NULL REFERENCES fraisier.deployments(id) ON DELETE CASCADE,
    service_name VARCHAR(255) NOT NULL,
    error_rate FLOAT DEFAULT 0.0,
    latency_p99 FLOAT DEFAULT 0.0,
    cpu_usage FLOAT DEFAULT 0.0,
    memory_usage FLOAT DEFAULT 0.0,
    active_connections INTEGER DEFAULT 0,
    recorded_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_deployment_metrics_deployment_id ON fraisier.deployment_metrics(deployment_id);
CREATE INDEX IF NOT EXISTS idx_deployment_metrics_service_name ON fraisier.deployment_metrics(service_name);
CREATE INDEX IF NOT EXISTS idx_deployment_metrics_recorded_at ON fraisier.deployment_metrics(recorded_at DESC);

-- ============================================================================
-- Services Table (for service configuration)
-- ============================================================================

CREATE TABLE IF NOT EXISTS fraisier.services (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name VARCHAR(255) NOT NULL UNIQUE,
    provider VARCHAR(50) NOT NULL,
    default_strategy VARCHAR(50) DEFAULT 'rolling',
    health_check_url VARCHAR(1000),
    health_check_type VARCHAR(50) DEFAULT 'http',
    enabled BOOLEAN DEFAULT TRUE,
    metadata JSONB DEFAULT '{}'::jsonb,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_services_name ON fraisier.services(name);
CREATE INDEX IF NOT EXISTS idx_services_provider ON fraisier.services(provider);
CREATE INDEX IF NOT EXISTS idx_services_enabled ON fraisier.services(enabled);

-- ============================================================================
-- Webhook Configurations Table
-- ============================================================================

CREATE TABLE IF NOT EXISTS fraisier.webhooks (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    service_name VARCHAR(255) NOT NULL,
    event_type VARCHAR(100) NOT NULL,
    url VARCHAR(1000) NOT NULL,
    method VARCHAR(10) DEFAULT 'POST',
    headers JSONB DEFAULT '{}'::jsonb,
    active BOOLEAN DEFAULT TRUE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_webhooks_service_name ON fraisier.webhooks(service_name);
CREATE INDEX IF NOT EXISTS idx_webhooks_event_type ON fraisier.webhooks(event_type);

-- ============================================================================
-- Rollback History Table
-- ============================================================================

CREATE TABLE IF NOT EXISTS fraisier.rollbacks (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    deployment_id UUID NOT NULL REFERENCES fraisier.deployments(id) ON DELETE CASCADE,
    service_name VARCHAR(255) NOT NULL,
    from_version VARCHAR(255) NOT NULL,
    to_version VARCHAR(255) NOT NULL,
    reason TEXT,
    timestamp TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_rollbacks_deployment_id ON fraisier.rollbacks(deployment_id);
CREATE INDEX IF NOT EXISTS idx_rollbacks_service_name ON fraisier.rollbacks(service_name);
CREATE INDEX IF NOT EXISTS idx_rollbacks_timestamp ON fraisier.rollbacks(timestamp DESC);

-- ============================================================================
-- Functions for automatic timestamp updates
-- ============================================================================

CREATE OR REPLACE FUNCTION fraisier.update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = CURRENT_TIMESTAMP;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER update_deployments_updated_at BEFORE UPDATE ON fraisier.deployments
    FOR EACH ROW EXECUTE FUNCTION fraisier.update_updated_at_column();

CREATE TRIGGER update_services_updated_at BEFORE UPDATE ON fraisier.services
    FOR EACH ROW EXECUTE FUNCTION fraisier.update_updated_at_column();

-- ============================================================================
-- Permissions for fraisier user
-- ============================================================================

GRANT SELECT, INSERT, UPDATE ON ALL TABLES IN SCHEMA fraisier TO fraisier;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA fraisier TO fraisier;
GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA fraisier TO fraisier;
