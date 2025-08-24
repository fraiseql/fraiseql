-- Blog Demo log_and_return_mutation function
-- Following PrintOptim core utility patterns

CREATE OR REPLACE FUNCTION core.log_and_return_mutation(
    input_actor UUID,
    input_entity_type TEXT,
    input_entity_id UUID,
    input_modification_type TEXT,
    input_change_status TEXT,
    input_fields TEXT[],
    input_message TEXT,
    input_payload_before JSONB DEFAULT NULL,
    input_payload_after JSONB DEFAULT NULL,
    input_extra_metadata JSONB DEFAULT '{}'::JSONB
)
RETURNS app.mutation_result
LANGUAGE plpgsql
AS $$
DECLARE
    v_return_result app.mutation_result;
BEGIN
    -- In a real implementation, this would log to a mutations table
    -- For blog demo, we'll skip the logging but maintain the interface

    v_return_result := (
        input_entity_id,
        input_fields,
        input_change_status,
        input_message,
        COALESCE(input_payload_after, input_payload_before),
        input_extra_metadata
    )::app.mutation_result;

    RETURN v_return_result;
END;
$$;

-- Simplified sanitize function for demo
CREATE OR REPLACE FUNCTION core.sanitize_jsonb_unset(input_data JSONB)
RETURNS JSONB
LANGUAGE plpgsql
AS $$
BEGIN
    -- For demo purposes, just return the input data
    -- In PrintOptim, this removes sensitive/internal fields
    RETURN input_data;
END;
$$;
