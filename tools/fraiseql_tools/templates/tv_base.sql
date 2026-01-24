-- Table-backed JSON view for {{ entity_name }}
-- Generated view name: tv_{{ view_name }}
-- Purpose: Materialized view storing JSON representations of {{ entity_name }} for efficient retrieval
-- Refresh strategy: {{ refresh_strategy }}

{% if not if_not_exists %}DROP TABLE IF EXISTS tv_{{ view_name }} CASCADE;{% endif %}

CREATE TABLE {% if if_not_exists %}IF NOT EXISTS{% endif %} tv_{{ view_name }} (
    -- View metadata
    view_id BIGSERIAL PRIMARY KEY,
    entity_id INTEGER NOT NULL UNIQUE,

    -- Payload storage
    entity_json JSONB NOT NULL DEFAULT '{}'::jsonb,

    -- Composition tracking (for nested relationship views)
    composition_ids TEXT[] DEFAULT ARRAY[]::TEXT[],

    -- Materialization metadata
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    view_generated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,

    -- Data quality tracking
    is_stale BOOLEAN DEFAULT false,
    staleness_detected_at TIMESTAMP WITH TIME ZONE,

    -- Indexing hints
    check_interval INTERVAL DEFAULT '1 hour'
);

-- Indexes for common access patterns
CREATE INDEX IF NOT EXISTS idx_tv_{{ view_name }}_entity_id
    ON tv_{{ view_name }}(entity_id);

CREATE INDEX IF NOT EXISTS idx_tv_{{ view_name }}_updated_at
    ON tv_{{ view_name }}(updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_tv_{{ view_name }}_is_stale
    ON tv_{{ view_name }}(is_stale)
    WHERE is_stale = true;

-- JSONB index for efficient JSON queries
CREATE INDEX IF NOT EXISTS idx_tv_{{ view_name }}_entity_json_gin
    ON tv_{{ view_name }} USING GIN(entity_json);

-- Composition tracking index
CREATE INDEX IF NOT EXISTS idx_tv_{{ view_name }}_composition_ids
    ON tv_{{ view_name }} USING GIN(composition_ids);

-- Comments for documentation
COMMENT ON TABLE tv_{{ view_name }} IS
    'Table-backed view storing {{ entity_name }} entities as JSONB for fast retrieval';
COMMENT ON COLUMN tv_{{ view_name }}.entity_id IS
    'Reference to the original entity ID in the source table';
COMMENT ON COLUMN tv_{{ view_name }}.entity_json IS
    'Complete JSON representation of the entity with all scalar and relationship fields';
COMMENT ON COLUMN tv_{{ view_name }}.is_stale IS
    'Flag indicating if this view entry needs refresh due to source data changes';
COMMENT ON COLUMN tv_{{ view_name }}.composition_ids IS
    'IDs of composed views that include this entity';
