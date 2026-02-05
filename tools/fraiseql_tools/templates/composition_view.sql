-- Composition helper views for {{ entity_name }} relationships
-- Purpose: Provide efficient queries for loading nested relationship data
-- These views support loading related entities for composition into parent views

{%- for relationship in relationships %}

-- Composition view for relationship: {{ relationship.name }}
{% if not if_not_exists %}DROP VIEW IF EXISTS cv_{{ entity_name }}_{{ relationship.name }} CASCADE;{% endif %}

CREATE VIEW {% if if_not_exists %}IF NOT EXISTS{% endif %} cv_{{ entity_name }}_{{ relationship.name }} AS
SELECT
    parent.entity_id AS parent_entity_id,
    related.entity_id AS related_entity_id,
    related.entity_json AS related_json,
    parent.updated_at AS parent_updated_at,
    related.updated_at AS related_updated_at
FROM
    tv_{{ entity_name }} parent
LEFT JOIN
    tv_{{ relationship.target_entity }} related
    ON parent.entity_id = related.entity_id
WHERE
    parent.view_generated_at IS NOT NULL
    AND (related.view_generated_at IS NOT NULL OR related.entity_id IS NULL);

COMMENT ON VIEW cv_{{ entity_name }}_{{ relationship.name }} IS
    'Composition view for loading related {{ relationship.target_entity }} entities for {{ entity_name }}.{{ relationship.name }}';

-- Index to optimize composition queries (using temporary table)
CREATE TEMPORARY TABLE IF NOT EXISTS comp_{{ entity_name }}_{{ relationship.name }}_work AS
SELECT parent_entity_id, array_agg(related_entity_id) as related_ids
FROM cv_{{ entity_name }}_{{ relationship.name }}
WHERE related_entity_id IS NOT NULL
GROUP BY parent_entity_id;

{%- endfor %}

-- Batch composition helper function
-- Efficiently loads related entities for a batch of parent IDs
CREATE OR REPLACE FUNCTION batch_compose_{{ entity_name }}(
    parent_ids INTEGER[]
)
RETURNS TABLE (
    parent_id INTEGER,
    entity_json JSONB,
    composed_json JSONB
) AS $$
BEGIN
    RETURN QUERY
    SELECT
        p.entity_id,
        p.entity_json,
        jsonb_build_object(
            {%- for relationship in relationships %}
            '{{ relationship.name }}', COALESCE(
                (
                    SELECT jsonb_agg(r.entity_json)
                    FROM tv_{{ relationship.target_entity }} r
                    WHERE r.entity_id = ANY(
                        SELECT related_entity_id
                        FROM cv_{{ entity_name }}_{{ relationship.name }}
                        WHERE parent_entity_id = p.entity_id
                    )
                ),
                'null'::jsonb
            )
            {%- if not loop.last %},{% endif %}
            {%- endfor %}
        ) AS composed
    FROM
        tv_{{ entity_name }} p
    WHERE
        p.entity_id = ANY(parent_ids)
    ORDER BY
        array_position(parent_ids, p.entity_id);
END;
$$ LANGUAGE plpgsql STABLE PARALLEL SAFE;

COMMENT ON FUNCTION batch_compose_{{ entity_name }}(INTEGER[]) IS
    'Batch composition helper for loading related {{ entity_name }} entities with all relationships';
