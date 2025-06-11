CREATE OR REPLACE FUNCTION testfoundry_cast_random_value(
    p_random_value TEXT,
    p_field_base_type TEXT
)
RETURNS JSONB
LANGUAGE plpgsql
AS $$
DECLARE
    v_casted_value JSONB;
BEGIN
    v_casted_value := CASE
        WHEN p_field_base_type IN ('int4', 'int8', 'integer', 'smallint', 'bigint')
            THEN to_jsonb(p_random_value::INTEGER)
        WHEN p_field_base_type IN ('float4', 'float8', 'numeric', 'real', 'double precision')
            THEN to_jsonb(p_random_value::FLOAT)
        WHEN p_field_base_type IN ('bool', 'boolean')
            THEN to_jsonb(p_random_value::BOOLEAN)
        WHEN p_field_base_type IN ('timestamp', 'timestamp without time zone', 'timestamp with time zone')
            THEN to_jsonb(p_random_value::TIMESTAMP)
        WHEN p_field_base_type = 'date'
            THEN to_jsonb(p_random_value::DATE)
        WHEN p_field_base_type = 'time'
            THEN to_jsonb(p_random_value::TIME)
        ELSE
            to_jsonb(p_random_value)
    END;
    RETURN v_casted_value;
END;
$$;
