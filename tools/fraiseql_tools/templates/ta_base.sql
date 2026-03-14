-- Table-backed Arrow view for {{ entity_name }}
-- Generated view name: ta_{{ view_name }}
-- Purpose: Materialized view storing Arrow-encoded columnar data for {{ entity_name }}
-- Refresh strategy: {{ refresh_strategy }}
-- Used by: Arrow Flight streaming and columnar bulk exports

{% if not if_not_exists %}DROP TABLE IF EXISTS ta_{{ view_name }} CASCADE;{% endif %}

CREATE TABLE {% if if_not_exists %}IF NOT EXISTS{% endif %} ta_{{ view_name }} (
    -- View metadata
    batch_id BIGSERIAL PRIMARY KEY,
    batch_number INTEGER NOT NULL,

    -- Arrow columnar storage
    -- Each column stores Arrow IPC-encoded RecordBatch for the field
    {%- for field in fields %}
    col_{{ field.name }} BYTEA NOT NULL DEFAULT ''::bytea,
    {%- endfor %}

    -- Batch metadata
    row_count INTEGER NOT NULL DEFAULT 0,
    batch_size_bytes BIGINT NOT NULL DEFAULT 0,
    compression CHAR(10) DEFAULT 'none',

    -- Flight metadata
    dictionary_encoded_fields TEXT[] DEFAULT ARRAY[]::TEXT[],
    field_compression_codecs TEXT[] DEFAULT ARRAY[]::TEXT[],

    -- Materialization metadata
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    view_generated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,

    -- Refresh tracking
    is_stale BOOLEAN DEFAULT false,
    staleness_detected_at TIMESTAMP WITH TIME ZONE,
    last_materialized_row_count BIGINT,

    -- Performance hints
    estimated_decode_time_ms INTEGER,
    check_interval INTERVAL DEFAULT '30 minutes'
);

-- Indexes for common access patterns
CREATE INDEX IF NOT EXISTS idx_ta_{{ view_name }}_batch_number
    ON ta_{{ view_name }}(batch_number DESC);

CREATE INDEX IF NOT EXISTS idx_ta_{{ view_name }}_updated_at
    ON ta_{{ view_name }}(updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_ta_{{ view_name }}_is_stale
    ON ta_{{ view_name }}(is_stale)
    WHERE is_stale = true;

CREATE INDEX IF NOT EXISTS idx_ta_{{ view_name }}_row_count
    ON ta_{{ view_name }}(row_count DESC);

-- Comments for documentation
COMMENT ON TABLE ta_{{ view_name }} IS
    'Table-backed Arrow view storing {{ entity_name }} entities as Arrow IPC RecordBatches for efficient columnar streaming';
COMMENT ON COLUMN ta_{{ view_name }}.batch_id IS
    'Unique identifier for this Arrow batch';
COMMENT ON COLUMN ta_{{ view_name }}.batch_number IS
    'Sequential batch number for ordering in Flight responses';
COMMENT ON COLUMN ta_{{ view_name }}.row_count IS
    'Number of rows encoded in this batch';
COMMENT ON COLUMN ta_{{ view_name }}.batch_size_bytes IS
    'Total size in bytes of Arrow-encoded data across all columns';
COMMENT ON COLUMN ta_{{ view_name }}.is_stale IS
    'Flag indicating if this view entry needs refresh due to source data changes';
COMMENT ON COLUMN ta_{{ view_name }}.compression IS
    'Compression codec used for Arrow buffers (none, snappy, lz4, zstd)';
