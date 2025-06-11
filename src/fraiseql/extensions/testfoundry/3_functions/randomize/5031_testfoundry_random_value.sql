/**
 * Function: testfoundry_random_value
 * -------------------------
 * Generates a random value based on the specified field type.
 *
 * Parameters:
 *   - p_field_type text: The type of the field for which a random value is to be generated.
 *
 * Returns:
 *   - text: A random value corresponding to the specified field type.
 *
 * Behavior:
 *   - Generates random values for various field types including UUID, UUID arrays, INET, MACADDR, boolean, integer, and timestamp.
 *   - For unsupported field types, returns a default random text value.
 *
 * Usage Examples:
 *   1. Generate a random UUID:
 *      SELECT testfoundry_random_value('uuid');
 *
 *      -- Output:
 *      -- A random UUID value, e.g., '550e8400-e29b-41d4-a716-446655440000'
 *
 *   2. Generate a random array of UUIDs:
 *      SELECT testfoundry_random_value('uuid[]');
 *
 *      -- Output:
 *      -- An array of random UUID values, e.g., '{550e8400-e29b-41d4-a716-446655440000, 550e8400-e29b-41d4-a716-446655440001}'
 *
 *   3. Generate a random INET value:
 *      SELECT testfoundry_random_value('inet');
 *
 *      -- Output:
 *      -- A random INET value, e.g., '192.168.123.45'
 *
 *   4. Generate a random MACADDR value:
 *      SELECT testfoundry_random_value('macaddr');
 *
 *      -- Output:
 *      -- A random MACADDR value, e.g., '01:23:45:67:89:AB'
 */

CREATE OR REPLACE FUNCTION testfoundry_random_value(p_field_type text)
RETURNS text
LANGUAGE plpgsql
AS $$
DECLARE
    v_base_type text;
    v_result text;
BEGIN
    -- Normalize type first
    v_base_type := lower(p_field_type);

    v_base_type := CASE
        WHEN v_base_type LIKE 'double precision%' THEN 'float'
        WHEN v_base_type LIKE 'numeric%' THEN 'numeric'
        WHEN v_base_type LIKE 'float%' THEN 'float'
        WHEN v_base_type LIKE 'integer%' OR v_base_type LIKE 'int%' THEN 'integer'
        WHEN v_base_type LIKE 'boolean%' THEN 'boolean'
        WHEN v_base_type LIKE 'timestamp%' THEN 'timestamp'
        WHEN v_base_type LIKE 'date%' THEN 'date'
        WHEN v_base_type LIKE 'time%' THEN 'time'
        WHEN v_base_type LIKE 'uuid[]' THEN 'uuid[]'
        WHEN v_base_type LIKE 'uuid' THEN 'uuid'
        WHEN v_base_type LIKE 'inet' THEN 'inet'
        WHEN v_base_type LIKE 'macaddr' THEN 'macaddr'
        ELSE 'text'
    END;

    -- Generate random value based on normalized type
    SELECT CASE v_base_type
        WHEN 'uuid' THEN gen_random_uuid()::text
        WHEN 'uuid[]' THEN (
            SELECT array_agg(gen_random_uuid())::text
            FROM generate_series(1, (1 + floor(random()*3))::int)
        )
        WHEN 'inet' THEN
            ('192.168.' || floor(random()*255)::int || '.' || floor(random()*255)::int)::text
        WHEN 'macaddr' THEN
            (lpad(to_hex(floor(random()*256)::int), 2, '0') || ':' ||
             lpad(to_hex(floor(random()*256)::int), 2, '0') || ':' ||
             lpad(to_hex(floor(random()*256)::int), 2, '0') || ':' ||
             lpad(to_hex(floor(random()*256)::int), 2, '0') || ':' ||
             lpad(to_hex(floor(random()*256)::int), 2, '0') || ':' ||
             lpad(to_hex(floor(random()*256)::int), 2, '0'))::text
        WHEN 'boolean' THEN
            (random() < 0.5)::text
        WHEN 'integer' THEN
            floor(random()*100)::int::text
        WHEN 'timestamp' THEN
            (now() - interval '5 years' + random() * interval '10 years')::timestamp::text
        WHEN 'date' THEN
            (current_date - interval '5 years' + (random() * interval '10 years'))::date::text
        WHEN 'time' THEN
            ('00:00'::time + (random() * interval '24 hours'))::time::text
        WHEN 'float' THEN
            (random() * 1000)::float::text
        ELSE
            ('test_' || substr(md5(random()::text), 1, 16))
    END
    INTO v_result;

    RETURN v_result;
END;
$$;



/**
 * Function: testfoundry_random_value
 * -------------------------
 * Generates a random value based on the specified field type.
 *
 * Parameters:
 *   - p_field_type text: The type of the field for which a random value is to be generated.
 *
 * Returns:
 *   - text: A random value corresponding to the specified field type.
 *
 * Behavior:
 *   - Generates random values for various field types including UUID, UUID arrays, INET, MACADDR, boolean, integer, and timestamp.
 *   - For unsupported field types, returns a default random text value.
 *
 * Usage Examples:
 *   1. Generate a random UUID:
 *      SELECT testfoundry_random_value('uuid');
 *
 *      -- Output:
 *      -- A random UUID value, e.g., '550e8400-e29b-41d4-a716-446655440000'
 *
 *   2. Generate a random array of UUIDs:
 *      SELECT testfoundry_random_value('uuid[]');
 *
 *      -- Output:
 *      -- An array of random UUID values, e.g., '{550e8400-e29b-41d4-a716-446655440000, 550e8400-e29b-41d4-a716-446655440001}'
 *
 *   3. Generate a random INET value:
 *      SELECT testfoundry_random_value('inet');
 *
 *      -- Output:
 *      -- A random INET value, e.g., '192.168.123.45'
 *
 *   4. Generate a random MACADDR value:
 *      SELECT testfoundry_random_value('macaddr');
 *
 *      -- Output:
 *      -- A random MACADDR value, e.g., '01:23:45:67:89:AB'
 */

CREATE OR REPLACE FUNCTION testfoundry_random_value(p_field_type text)
RETURNS text
LANGUAGE plpgsql
AS $$
DECLARE
    v_base_type text;
    v_result text;
BEGIN
    -- Normalize type first
    v_base_type := lower(p_field_type);

    v_base_type := CASE
        WHEN v_base_type LIKE 'double precision%' THEN 'float'
        WHEN v_base_type LIKE 'numeric%' THEN 'numeric'
        WHEN v_base_type LIKE 'float%' THEN 'float'
        WHEN v_base_type LIKE 'integer%' OR v_base_type LIKE 'int%' THEN 'integer'
        WHEN v_base_type LIKE 'boolean%' THEN 'boolean'
        WHEN v_base_type LIKE 'timestamp%' THEN 'timestamp'
        WHEN v_base_type LIKE 'date%' THEN 'date'
        WHEN v_base_type LIKE 'time%' THEN 'time'
        WHEN v_base_type LIKE 'uuid[]' THEN 'uuid[]'
        WHEN v_base_type LIKE 'uuid' THEN 'uuid'
        WHEN v_base_type LIKE 'inet' THEN 'inet'
        WHEN v_base_type LIKE 'macaddr' THEN 'macaddr'
        ELSE 'text'
    END;

    -- Generate random value based on normalized type
    SELECT CASE v_base_type
        WHEN 'uuid' THEN gen_random_uuid()::text
        WHEN 'uuid[]' THEN (
            SELECT array_agg(gen_random_uuid())::text
            FROM generate_series(1, (1 + floor(random()*3))::int)
        )
        WHEN 'inet' THEN
            ('192.168.' || floor(random()*255)::int || '.' || floor(random()*255)::int)::text
        WHEN 'macaddr' THEN
            (lpad(to_hex(floor(random()*256)::int), 2, '0') || ':' ||
             lpad(to_hex(floor(random()*256)::int), 2, '0') || ':' ||
             lpad(to_hex(floor(random()*256)::int), 2, '0') || ':' ||
             lpad(to_hex(floor(random()*256)::int), 2, '0') || ':' ||
             lpad(to_hex(floor(random()*256)::int), 2, '0') || ':' ||
             lpad(to_hex(floor(random()*256)::int), 2, '0'))::text
        WHEN 'boolean' THEN
            (random() < 0.5)::text
        WHEN 'integer' THEN
            floor(random()*100)::int::text
        WHEN 'timestamp' THEN
            (now() - interval '5 years' + random() * interval '10 years')::timestamp::text
        WHEN 'date' THEN
            (current_date - interval '5 years' + (random() * interval '10 years'))::date::text
        WHEN 'time' THEN
            ('00:00'::time + (random() * interval '24 hours'))::time::text
        WHEN 'float' THEN
            (random() * 1000)::float::text
        ELSE
            ('test_' || substr(md5(random()::text), 1, 16))
    END
    INTO v_result;

    RETURN v_result;
END;
$$;
