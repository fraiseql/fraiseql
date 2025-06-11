/**
 * Function: <function_name>
 * -------------------------
 * <Short description of what this function does.>
 *
 * Parameters:
 *   - <param_name> <param_type> [DEFAULT <default_value>]: <Brief description of the parameter>
 *   - ...
 *
 * Returns:
 *   - <return_type>: <Description of what the function returns>
 *
 * Behavior:
 *   - <Optional, bullet points describing the important behavior or side effects>
 *
 * Usage Examples:
 *   1. <Example 1 Title>:
 *      SELECT <function_name>(<params>);
 *
 *      -- Output:
 *      <example output>
 *
 *   2. <Example 2 Title>:
 *      SELECT <function_name>(<params>);
 *
 *      -- Output:
 *      <example output>
 */


-- Full working version of TestFoundry random entity generator focusing only on json_data field

CREATE OR REPLACE FUNCTION testfoundry_random_entity(p_entity_name text)
RETURNS jsonb
LANGUAGE plpgsql
AS $$
DECLARE
    v_json_data text;
BEGIN
    v_json_data := testfoundry_random_jsonb_build(p_entity_name);
    RETURN v_json_data::jsonb;
END;
$$;

-- Helper function to dynamically generate random json_data based on view definition
CREATE OR REPLACE FUNCTION testfoundry_random_jsonb_build(p_entity_name text)
RETURNS text
LANGUAGE plpgsql
AS $$
DECLARE
    v_definition text;
    v_json_data text := '{';
    r record;
    v_key text;
    v_source text;
    v_pairs text[];
    i int;
BEGIN
    SELECT pg_get_viewdef('v_' || p_entity_name, true) INTO v_definition;

    -- Extract the jsonb_build_object content
    v_definition := substring(v_definition FROM 'jsonb_build_object\((.*)\)');

    -- Split by commas carefully into key-value pairs
    v_pairs := string_to_array(v_definition, ',');

    i := 1;
    WHILE i < array_length(v_pairs, 1) LOOP
        v_key := trim(both ' ''"' FROM v_pairs[i]);
        v_source := trim(v_pairs[i+1]);

        -- Heuristic generation based on source name using testfoundry_random_value
        IF v_source ILIKE '%pk_%' THEN
            -- 1. Primary key fields → UUID
            v_json_data := v_json_data || format('"%s": "%s",', v_key, testfoundry_random_value('uuid'));
        ELSIF v_key ~ '^n_[a-z]' OR v_key ILIKE '%_count' OR v_key ILIKE '%_total%' THEN
            -- 2. Numeric fields → Integer
            v_json_data := v_json_data || format('"%s": %s,', v_key, (random()*10)::int);
        ELSIF v_source ILIKE '%id%' THEN
            -- 3. ID fields → UUID
            v_json_data := v_json_data || format('"%s": "%s",', v_key, testfoundry_random_value('uuid'));
        ELSIF v_key ILIKE '%identifier%' THEN
            -- 4. Identifier fields → Text
            v_json_data := v_json_data || format('"%s": "%s",', v_key, testfoundry_random_value('text'));
        ELSIF v_key ILIKE '%ip_address%' THEN
            -- 5. IP address fields → Inet
            v_json_data := v_json_data || format('"%s": "%s",', v_key, testfoundry_random_value('inet'));
        ELSIF v_key ILIKE '%mac_addr%' OR v_key ILIKE '%macaddress%' OR v_key ILIKE '%mac_address%' THEN
            -- 6. MAC address fields → MAC
            v_json_data := v_json_data || format('"%s": "%s",', v_key, testfoundry_random_value('macaddr'));
        ELSE
            -- 7. Default → Text
            v_json_data := v_json_data || format('"%s": "%s",', v_key, testfoundry_random_value('text'));
        END IF;

            i := i + 2;
        END LOOP;

    -- Remove trailing comma
    v_json_data := regexp_replace(v_json_data, ',\s*$', '', 'g');

    v_json_data := v_json_data || '}';
    RETURN v_json_data;
END;
$$;
