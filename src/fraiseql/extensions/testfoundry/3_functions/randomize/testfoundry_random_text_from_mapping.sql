CREATE OR REPLACE FUNCTION testfoundry_random_text_from_mapping(
    p_input_type TEXT,
    p_args TEXT[] DEFAULT NULL
)
RETURNS TEXT
AS $$
DECLARE
    v_result JSONB;
    v_text TEXT;
BEGIN
    IF p_args IS NULL THEN
        SELECT testfoundry_random_value_from_mapping(p_input_type, ARRAY[]::TEXT[]) INTO v_result;
    ELSE
        SELECT testfoundry_random_value_from_mapping(p_input_type, p_args) INTO v_result;
    END IF;

    -- If no result, return NULL
    IF v_result IS NULL THEN
        RETURN NULL;
    END IF;

    -- Clean JSONB scalar → TEXT
    v_text := trim(BOTH '"' FROM v_result::TEXT);

    RETURN v_text;
END;
$$ LANGUAGE plpgsql STABLE;
