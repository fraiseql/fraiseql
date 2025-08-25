-- Blog Demo Enterprise Mutation Result Handler
-- Standardized function for mutation results with logging

CREATE OR REPLACE FUNCTION app.log_and_return_mutation(
    p_success BOOLEAN,
    p_message TEXT,
    p_object_data JSONB DEFAULT NULL,
    p_error_code TEXT DEFAULT NULL,
    p_metadata JSONB DEFAULT NULL,
    p_function_name TEXT DEFAULT NULL,
    p_user_id UUID DEFAULT NULL,
    p_tenant_id UUID DEFAULT NULL
) RETURNS mutation_result AS $$
DECLARE
    v_result mutation_result;
    v_log_entry JSONB;
BEGIN
    -- Build the result
    v_result.success := p_success;
    v_result.message := p_message;
    v_result.object_data := p_object_data;
    v_result.error_code := p_error_code;
    v_result.metadata := COALESCE(p_metadata, '{}');

    -- Add execution metadata
    v_result.metadata := v_result.metadata || jsonb_build_object(
        'executed_at', NOW(),
        'function_name', COALESCE(p_function_name, 'unknown'),
        'tenant_id', p_tenant_id,
        'user_id', p_user_id
    );

    -- Create log entry for audit trail
    v_log_entry := jsonb_build_object(
        'timestamp', NOW(),
        'function_name', COALESCE(p_function_name, 'unknown'),
        'success', p_success,
        'message', p_message,
        'error_code', p_error_code,
        'tenant_id', p_tenant_id,
        'user_id', p_user_id,
        'object_type', CASE
            WHEN p_object_data IS NOT NULL AND p_object_data ? 'pk_organization' THEN 'organization'
            WHEN p_object_data IS NOT NULL AND p_object_data ? 'pk_user' THEN 'user'
            WHEN p_object_data IS NOT NULL AND p_object_data ? 'pk_post' THEN 'post'
            WHEN p_object_data IS NOT NULL AND p_object_data ? 'pk_comment' THEN 'comment'
            WHEN p_object_data IS NOT NULL AND p_object_data ? 'pk_tag' THEN 'tag'
            ELSE 'unknown'
        END
    );

    -- TODO: In a real system, you might want to log to a dedicated audit table
    -- For now, we'll use RAISE NOTICE for debugging
    IF NOT p_success THEN
        RAISE NOTICE 'Mutation failed: % (%) - %', p_function_name, p_error_code, p_message;
    END IF;

    RETURN v_result;
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;
