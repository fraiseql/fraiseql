-- Analytics Dashboard Database Schema
-- Demonstrates FraiseQL's capabilities with time-series data and complex aggregations

-- Enable necessary extensions
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "timescaledb" CASCADE; -- For time-series optimization (optional)

-- Organizations/Companies
CREATE TABLE organizations (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name VARCHAR(255) NOT NULL,
    slug VARCHAR(100) NOT NULL UNIQUE,
    domain VARCHAR(100),
    industry VARCHAR(100),
    country_code VARCHAR(2),
    timezone VARCHAR(50) DEFAULT 'UTC',
    settings JSONB DEFAULT '{}',
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- Applications/Products being tracked
CREATE TABLE applications (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    slug VARCHAR(100) NOT NULL,
    description TEXT,
    app_type VARCHAR(50) DEFAULT 'web', -- web, mobile, api, etc.
    platform VARCHAR(50), -- ios, android, web, etc.
    version VARCHAR(50),
    is_active BOOLEAN DEFAULT true,
    settings JSONB DEFAULT '{}',
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(organization_id, slug)
);

-- User sessions for tracking
CREATE TABLE user_sessions (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    application_id UUID NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
    session_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(255), -- External user identifier
    device_type VARCHAR(50), -- desktop, mobile, tablet
    browser VARCHAR(100),
    os VARCHAR(100),
    country_code VARCHAR(2),
    region VARCHAR(100),
    city VARCHAR(100),
    ip_address INET,
    user_agent TEXT,
    referrer TEXT,
    utm_source VARCHAR(100),
    utm_medium VARCHAR(100),
    utm_campaign VARCHAR(100),
    utm_content VARCHAR(100),
    utm_term VARCHAR(100),
    started_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    ended_at TIMESTAMP WITH TIME ZONE,
    duration_seconds INTEGER,
    page_views INTEGER DEFAULT 0,
    is_bounce BOOLEAN DEFAULT false,
    metadata JSONB DEFAULT '{}'
);

-- Page views and events
CREATE TABLE page_views (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    application_id UUID NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
    session_id UUID REFERENCES user_sessions(id) ON DELETE CASCADE,
    user_id VARCHAR(255),
    page_url TEXT NOT NULL,
    page_title VARCHAR(500),
    page_path VARCHAR(500),
    query_params JSONB DEFAULT '{}',
    referrer TEXT,
    load_time_ms INTEGER,
    time_on_page_seconds INTEGER,
    scroll_depth_percent INTEGER,
    exit_page BOOLEAN DEFAULT false,
    bounce BOOLEAN DEFAULT false,
    timestamp TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    metadata JSONB DEFAULT '{}'
);

-- Custom events (clicks, form submissions, etc.)
CREATE TABLE events (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    application_id UUID NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
    session_id UUID REFERENCES user_sessions(id) ON DELETE CASCADE,
    user_id VARCHAR(255),
    event_name VARCHAR(255) NOT NULL,
    event_category VARCHAR(100),
    event_action VARCHAR(100),
    event_label VARCHAR(255),
    event_value DECIMAL(10,2),
    page_url TEXT,
    element_selector VARCHAR(500),
    properties JSONB DEFAULT '{}',
    timestamp TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- Conversion goals and funnels
CREATE TABLE conversion_goals (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    application_id UUID NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    description TEXT,
    goal_type VARCHAR(50) NOT NULL, -- url, event, duration
    goal_value TEXT, -- URL pattern, event name, etc.
    value_amount DECIMAL(10,2), -- Monetary value of conversion
    is_active BOOLEAN DEFAULT true,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- Conversion tracking
CREATE TABLE conversions (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    application_id UUID NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
    goal_id UUID NOT NULL REFERENCES conversion_goals(id) ON DELETE CASCADE,
    session_id UUID REFERENCES user_sessions(id) ON DELETE CASCADE,
    user_id VARCHAR(255),
    conversion_value DECIMAL(10,2),
    attribution_source VARCHAR(100),
    attribution_medium VARCHAR(100),
    attribution_campaign VARCHAR(100),
    time_to_conversion_seconds INTEGER,
    timestamp TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    metadata JSONB DEFAULT '{}'
);

-- A/B tests and experiments
CREATE TABLE experiments (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    application_id UUID NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    description TEXT,
    hypothesis TEXT,
    status VARCHAR(50) DEFAULT 'draft', -- draft, running, completed, archived
    traffic_allocation DECIMAL(5,2) DEFAULT 100.00, -- Percentage of traffic
    start_date TIMESTAMP WITH TIME ZONE,
    end_date TIMESTAMP WITH TIME ZONE,
    primary_metric VARCHAR(255),
    significance_level DECIMAL(5,2) DEFAULT 95.00,
    settings JSONB DEFAULT '{}',
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- A/B test variants
CREATE TABLE experiment_variants (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    experiment_id UUID NOT NULL REFERENCES experiments(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    description TEXT,
    is_control BOOLEAN DEFAULT false,
    traffic_weight DECIMAL(5,2) DEFAULT 50.00, -- Percentage of experiment traffic
    configuration JSONB DEFAULT '{}',
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- A/B test assignments
CREATE TABLE experiment_assignments (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    experiment_id UUID NOT NULL REFERENCES experiments(id) ON DELETE CASCADE,
    variant_id UUID NOT NULL REFERENCES experiment_variants(id) ON DELETE CASCADE,
    session_id UUID REFERENCES user_sessions(id) ON DELETE SET NULL,
    user_id VARCHAR(255),
    assigned_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(experiment_id, session_id, user_id)
);

-- Performance metrics
CREATE TABLE performance_metrics (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    application_id UUID NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
    session_id UUID REFERENCES user_sessions(id) ON DELETE CASCADE,
    metric_name VARCHAR(100) NOT NULL,
    metric_value DECIMAL(10,3) NOT NULL,
    metric_unit VARCHAR(20), -- ms, seconds, bytes, etc.
    page_url TEXT,
    browser VARCHAR(100),
    device_type VARCHAR(50),
    connection_type VARCHAR(50),
    timestamp TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    metadata JSONB DEFAULT '{}'
);

-- Error tracking
CREATE TABLE error_events (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    application_id UUID NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
    session_id UUID REFERENCES user_sessions(id) ON DELETE CASCADE,
    user_id VARCHAR(255),
    error_type VARCHAR(100) NOT NULL, -- javascript, http, etc.
    error_message TEXT,
    error_stack TEXT,
    page_url TEXT,
    browser VARCHAR(100),
    device_type VARCHAR(50),
    severity VARCHAR(20) DEFAULT 'error', -- debug, info, warning, error, critical
    resolved BOOLEAN DEFAULT false,
    timestamp TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    metadata JSONB DEFAULT '{}'
);

-- Revenue and financial metrics
CREATE TABLE revenue_events (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    application_id UUID NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
    session_id UUID REFERENCES user_sessions(id) ON DELETE CASCADE,
    user_id VARCHAR(255),
    transaction_id VARCHAR(255),
    revenue_amount DECIMAL(12,2) NOT NULL,
    currency_code VARCHAR(3) DEFAULT 'USD',
    product_category VARCHAR(100),
    product_name VARCHAR(255),
    quantity INTEGER DEFAULT 1,
    discount_amount DECIMAL(12,2) DEFAULT 0,
    tax_amount DECIMAL(12,2) DEFAULT 0,
    payment_method VARCHAR(50),
    timestamp TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    metadata JSONB DEFAULT '{}'
);

-- Indexes for performance
CREATE INDEX idx_user_sessions_app_started ON user_sessions(application_id, started_at DESC);
CREATE INDEX idx_user_sessions_user ON user_sessions(user_id) WHERE user_id IS NOT NULL;
CREATE INDEX idx_user_sessions_session ON user_sessions(session_id);

CREATE INDEX idx_page_views_app_timestamp ON page_views(application_id, timestamp DESC);
CREATE INDEX idx_page_views_session ON page_views(session_id);
CREATE INDEX idx_page_views_url ON page_views(application_id, page_path);

CREATE INDEX idx_events_app_timestamp ON events(application_id, timestamp DESC);
CREATE INDEX idx_events_name ON events(application_id, event_name);
CREATE INDEX idx_events_session ON events(session_id);

CREATE INDEX idx_conversions_app_timestamp ON conversions(application_id, timestamp DESC);
CREATE INDEX idx_conversions_goal ON conversions(goal_id);

CREATE INDEX idx_performance_app_timestamp ON performance_metrics(application_id, timestamp DESC);
CREATE INDEX idx_performance_metric ON performance_metrics(application_id, metric_name);

CREATE INDEX idx_error_events_app_timestamp ON error_events(application_id, timestamp DESC);
CREATE INDEX idx_error_events_resolved ON error_events(application_id, resolved, timestamp DESC);

CREATE INDEX idx_revenue_app_timestamp ON revenue_events(application_id, timestamp DESC);
CREATE INDEX idx_revenue_user ON revenue_events(user_id) WHERE user_id IS NOT NULL;

-- TimescaleDB hypertables (if TimescaleDB is available)
DO $$
BEGIN
    IF EXISTS (SELECT FROM pg_extension WHERE extname = 'timescaledb') THEN
        -- Convert time-series tables to hypertables
        PERFORM create_hypertable('user_sessions', 'started_at', chunk_time_interval => INTERVAL '1 day');
        PERFORM create_hypertable('page_views', 'timestamp', chunk_time_interval => INTERVAL '1 day');
        PERFORM create_hypertable('events', 'timestamp', chunk_time_interval => INTERVAL '1 day');
        PERFORM create_hypertable('conversions', 'timestamp', chunk_time_interval => INTERVAL '1 day');
        PERFORM create_hypertable('performance_metrics', 'timestamp', chunk_time_interval => INTERVAL '1 day');
        PERFORM create_hypertable('error_events', 'timestamp', chunk_time_interval => INTERVAL '1 day');
        PERFORM create_hypertable('revenue_events', 'timestamp', chunk_time_interval => INTERVAL '1 day');
    END IF;
EXCEPTION
    WHEN OTHERS THEN
        -- TimescaleDB not available, continue without hypertables
        NULL;
END $$;

-- Update triggers
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = CURRENT_TIMESTAMP;
    RETURN NEW;
END;
$$ language 'plpgsql';

CREATE TRIGGER update_organizations_updated_at BEFORE UPDATE ON organizations
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_applications_updated_at BEFORE UPDATE ON applications
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_conversion_goals_updated_at BEFORE UPDATE ON conversion_goals
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_experiments_updated_at BEFORE UPDATE ON experiments
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- Session duration calculation trigger
CREATE OR REPLACE FUNCTION calculate_session_duration()
RETURNS TRIGGER AS $$
BEGIN
    IF NEW.ended_at IS NOT NULL AND OLD.ended_at IS NULL THEN
        NEW.duration_seconds = EXTRACT(EPOCH FROM (NEW.ended_at - NEW.started_at));

        -- Mark as bounce if session was very short with only 1 page view
        IF NEW.duration_seconds < 10 AND NEW.page_views <= 1 THEN
            NEW.is_bounce = true;
        END IF;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER session_duration_trigger BEFORE UPDATE ON user_sessions
    FOR EACH ROW EXECUTE FUNCTION calculate_session_duration();

-- Real-time analytics materialized views refresh
CREATE OR REPLACE FUNCTION refresh_analytics_views()
RETURNS VOID AS $$
BEGIN
    -- This function can be called periodically to refresh materialized views
    -- Implementation depends on which views need refreshing
    NULL;
END;
$$ LANGUAGE plpgsql;
