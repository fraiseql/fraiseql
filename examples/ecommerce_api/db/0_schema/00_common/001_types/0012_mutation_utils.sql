-- Mutation utilities
-- Centralized Debezium-style response formatting

-- Ultra-direct mutation response formatter
-- Returns simple JSONB for Rust transformer (no Debezium complexity)
CREATE OR REPLACE FUNCTION app.build_mutation_response(
    success BOOLEAN,
    code TEXT,
    message TEXT,
    data JSONB DEFAULT NULL
) RETURNS JSONB AS $$
BEGIN
    RETURN jsonb_build_object(
        'success', success,
        'code', code,
        'message', message
    ) || COALESCE(data, '{}'::jsonb);
END;
$$ LANGUAGE plpgsql;
