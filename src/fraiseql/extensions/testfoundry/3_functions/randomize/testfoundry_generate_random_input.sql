-- ============================================================================
-- Function: testfoundry_generate_random_input
-- Purpose:
--   Dynamically generate a random JSONB object for a given composite input type.
-- ============================================================================

CREATE OR REPLACE FUNCTION testfoundry_generate_random_input(
    p_entity TEXT,
    p_debug BOOLEAN DEFAULT FALSE
)
RETURNS JSONB
LANGUAGE plpgsql
AS $$
DECLARE
    v_json JSONB := '{}';
    v_field RECORD;
    v_generated_groups TEXT[] := ARRAY[]::TEXT[];
    v_generated_fields TEXT[] := ARRAY[]::TEXT[];
    v_temp_json JSONB;
    v_fk_args TEXT[];
    v_dep_field TEXT;
    v_value JSONB;
    v_dependency_missing BOOLEAN;
    v_pending_fields INT;
    v_last_pending_fields INT := -1;
BEGIN
    -- Loop until all fields are generated or no progress
    LOOP
        v_pending_fields := 0;

        FOR v_field IN
            SELECT *
            FROM testfoundry_list_input_fields(p_entity)
        LOOP
            -- 🔥 Dynamic fallback: Fill missing dependency info if needed
            IF v_field.generator_type = 'resolve_fk' AND v_field.fk_dependency_fields IS NULL THEN
                SELECT dependency_fields
                INTO v_field.fk_dependency_fields
                FROM testfoundry_tb_fk_mapping
                WHERE input_type = v_field.fk_mapping_key;
            END IF;

            -- Skip if field belongs to an already generated group
            IF v_field.generator_group IS NOT NULL
               AND v_field.generator_group = ANY(v_generated_groups) THEN
                CONTINUE;
            END IF;

            -- Skip if field already individually generated
            IF v_field.field_name = ANY(v_generated_fields) THEN
                CONTINUE;
            END IF;

            -- Check if field dependencies are satisfied
            v_dependency_missing := FALSE;

            IF v_field.fk_dependency_fields IS NOT NULL THEN
                FOREACH v_dep_field IN ARRAY v_field.fk_dependency_fields
                LOOP
                    IF NOT (v_json ? v_dep_field OR v_json ? ('fk_' || v_dep_field)) THEN
                        v_dependency_missing := TRUE;
                        EXIT;
                    END IF;
                END LOOP;
            END IF;

            IF v_dependency_missing THEN
                v_pending_fields := v_pending_fields + 1;
                IF p_debug THEN
                    RAISE NOTICE 'Skipping field "%" (waiting for dependencies)', v_field.field_name;
                END IF;
                CONTINUE;
            END IF;

            -- Handle group leader generation
            IF v_field.group_leader IS TRUE THEN
                v_fk_args := ARRAY[]::TEXT[];

                IF v_field.fk_dependency_fields IS NOT NULL THEN
                    FOREACH v_dep_field IN ARRAY v_field.fk_dependency_fields
                    LOOP
                        IF v_json ? ('fk_' || v_dep_field) THEN
                            v_fk_args := array_append(v_fk_args, v_json ->> ('fk_' || v_dep_field));
                        ELSIF v_json ? v_dep_field THEN
                            v_fk_args := array_append(v_fk_args, v_json ->> v_dep_field);
                        END IF;
                    END LOOP;
                END IF;

                IF p_debug THEN
                    RAISE NOTICE 'Generating group leader "%" with mapping "%"', v_field.field_name, v_field.fk_mapping_key;
                END IF;

                IF array_length(array_remove(v_fk_args, NULL), 1) > 0 THEN
                    SELECT testfoundry_random_value_from_mapping(v_field.fk_mapping_key, VARIADIC array_remove(v_fk_args, NULL))
                    INTO v_temp_json;
                ELSE
                    SELECT testfoundry_random_value_from_mapping(v_field.fk_mapping_key)
                    INTO v_temp_json;
                END IF;

                IF jsonb_typeof(v_temp_json) = 'object' THEN
                    FOR v_dep_field IN SELECT jsonb_object_keys(v_temp_json)
                    LOOP
                        v_json := v_json || jsonb_build_object(v_dep_field, v_temp_json ->> v_dep_field);
                        v_generated_fields := array_append(v_generated_fields, v_dep_field); -- ✅ mark fields generated
                    END LOOP;
                ELSE
                    v_json := v_json || jsonb_build_object(v_field.field_name, v_temp_json);
                    v_generated_fields := array_append(v_generated_fields, v_field.field_name); -- ✅ mark field generated
                END IF;

                v_generated_groups := array_append(v_generated_groups, v_field.generator_group);
                CONTINUE;
            END IF;

            -- Normal field generation
            IF p_debug THEN
                RAISE NOTICE 'Generating field "%" of type "%"', v_field.field_name, v_field.generator_type;
            END IF;

            v_value := testfoundry_generate_field_value(v_field, v_json);

            IF v_field.generator_type = 'resolve_fk'
               AND v_field.random_pk_field IS NOT NULL
               AND v_field.random_value_field IS NOT NULL THEN

                v_json := v_json
                    || jsonb_build_object(v_field.field_name, v_value ->> 'value')
                    || jsonb_build_object('fk_' || v_field.field_name, v_value ->> 'pk');
            ELSE
                v_json := v_json || jsonb_build_object(v_field.field_name, v_value);
            END IF;

            -- ✅ After successful normal field generation
            v_generated_fields := array_append(v_generated_fields, v_field.field_name);

        END LOOP;

        -- Exit conditions
        IF v_pending_fields = 0 THEN
            EXIT; -- All fields generated
        END IF;

        IF v_pending_fields = v_last_pending_fields THEN
            RAISE EXCEPTION 'Cannot resolve all field dependencies for entity: %', p_entity;
        END IF;

        v_last_pending_fields := v_pending_fields;
    END LOOP;

    RETURN v_json;
END;
$$;
