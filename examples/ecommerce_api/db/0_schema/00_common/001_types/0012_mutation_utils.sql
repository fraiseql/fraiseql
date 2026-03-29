-- Mutation utilities
-- Centralized mutation response formatting

-- Helper: build a success mutation_response
CREATE OR REPLACE FUNCTION app.build_mutation_response(
    p_status TEXT,
    p_message TEXT,
    p_entity JSONB DEFAULT NULL,
    p_entity_type TEXT DEFAULT NULL,
    p_entity_id TEXT DEFAULT NULL,
    p_cascade JSONB DEFAULT NULL,
    p_metadata JSONB DEFAULT NULL
) RETURNS mutation_response AS $$
BEGIN
    RETURN ROW(
        p_status,
        p_message,
        p_entity_id,
        p_entity_type,
        p_entity,
        NULL::text[],
        p_cascade,
        p_metadata
    )::mutation_response;
END;
$$ LANGUAGE plpgsql IMMUTABLE;
