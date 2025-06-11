CREATE OR REPLACE FUNCTION testfoundry_select_pk_value(
    p_pk_field TEXT,
    p_value_field TEXT
)
RETURNS TEXT
LANGUAGE SQL
AS $$
    SELECT
        'jsonb_build_object(' ||
        quote_literal('pk') || ', ' || p_pk_field || ', ' ||
        quote_literal('value') || ', ' || p_value_field ||
        ')'
$$;
