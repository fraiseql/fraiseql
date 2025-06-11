/**
 * Function: insert_entity
 * ------------------------
 * Inserts a record into an entity table, respecting timing, duration constraints,
 * field overrides, and custom timing fields. If a corresponding info table exists,
 * populates it first and links the base entity to the info record.
 *
 * Parameters:
 *   - p_entity TEXT: The entity to insert into (without "tb_" prefix).
 *   - p_timing TEXT: 'past', 'current', or 'future' (defines base timing window).
 *   - p_schema TEXT DEFAULT 'public': Schema name.
 *   - p_duration_constraints TEXT[] DEFAULT NULL: Optional constraints like '> 3 months', '< 4 years'.
 *   - p_overrides JSONB DEFAULT NULL: Optional JSONB object {field_name: value} to manually override field values.
 *   - p_reference_delta INTERVAL DEFAULT NULL: Optional fixed interval to determine start_date/end_date relative to now.
 *
 * Returns:
 *   - UUID of the inserted record.
 */

CREATE OR REPLACE FUNCTION insert_entity(
    p_entity TEXT,
    p_timing TEXT,
    p_schema TEXT DEFAULT 'public',
    p_duration_constraints TEXT[] DEFAULT NULL,
    p_overrides JSONB DEFAULT NULL,
    p_reference_delta INTERVAL DEFAULT NULL
)
RETURNS UUID
LANGUAGE plpgsql
AS $$
DECLARE
    v_table TEXT;
    v_pk_field TEXT;
    v_info_table TEXT;
    v_new_id UUID := gen_random_uuid();
    v_info_id UUID := gen_random_uuid();
    v_parent RECORD;
    v_start_date DATE;
    v_end_date DATE;
    v_min_duration INTERVAL := INTERVAL '1 month';
    v_max_duration INTERVAL := INTERVAL '5 years';
    r_constraint TEXT;
    r_parent RECORD;
    v_field_list TEXT := '';
    v_value_list TEXT := '';
    v_info_field_list TEXT := '';
    v_info_value_list TEXT := '';
    r_field RECORD;
    v_start_field TEXT := 'start_date';
    v_end_field TEXT := 'end_date';
    v_info_exists BOOLEAN;
    v_random_interval INTERVAL;
BEGIN
    -- Determine table and primary key field
    v_table := format('%I.tb_%s', p_schema, p_entity);
    v_pk_field := format('pk_%s', p_entity);
    v_info_table := format('%I.tb_%s_info', p_schema, p_entity);

    -- Check if info table exists
    SELECT EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_schema = p_schema
          AND table_name = format('tb_%s_info', p_entity)
    ) INTO v_info_exists;

    -- Determine if custom timing fields exist
    SELECT start_field, end_field INTO v_start_field, v_end_field
    FROM public.test_entity_timing_fields
    WHERE entity = p_entity
    LIMIT 1;

    -- Apply duration constraints if any
    IF p_duration_constraints IS NOT NULL THEN
        FOREACH r_constraint IN ARRAY p_duration_constraints LOOP
            IF r_constraint LIKE '> %' THEN
                v_min_duration := substring(r_constraint FROM 3)::interval;
            ELSIF r_constraint LIKE '< %' THEN
                v_max_duration := substring(r_constraint FROM 3)::interval;
            END IF;
        END LOOP;
    END IF;

    -- Random duration within limits
    v_random_interval := v_min_duration + (random() * (v_max_duration - v_min_duration));

    -- Determine dates based on timing and duration constraints
    CASE lower(p_timing)
        WHEN 'past' THEN
            IF p_reference_delta IS NULL THEN
                v_end_date := current_date - (interval '1 month' + (random() * interval '6 months'));
            ELSE
                v_end_date := current_date - p_reference_delta;
            END IF;
            v_start_date := v_end_date - v_random_interval;

        WHEN 'current' THEN
            IF p_reference_delta IS NULL THEN
                v_start_date := current_date - (interval '15 days' + (random() * interval '15 days'));
                v_end_date := v_start_date + v_random_interval;
            ELSE
                v_start_date := current_date - p_reference_delta;
                v_end_date := current_date + p_reference_delta;
            END IF;

        WHEN 'future' THEN
            IF p_reference_delta IS NULL THEN
                v_start_date := current_date + (interval '1 month' + (random() * interval '12 months'));
            ELSE
                v_start_date := current_date + p_reference_delta;
            END IF;
            v_end_date := v_start_date + v_random_interval;

        ELSE
            RAISE EXCEPTION 'Invalid timing option: %', p_timing;
    END CASE;

    -- Optionally insert into info table fully populated first
    IF v_info_exists THEN
        v_info_field_list := format('pk_%s_info', p_entity);
        v_info_value_list := format('''%s''', v_info_id);

        FOR r_field IN
            SELECT attname
            FROM pg_attribute
            WHERE attrelid = format('%I.tb_%s_info', p_schema, p_entity)::regclass
              AND attnum > 0
              AND NOT attisdropped
              AND attname NOT IN (format('pk_%s_info', p_entity), 'created_at', 'updated_at')
        LOOP
            v_info_field_list := v_info_field_list || format(', %I', r_field.attname);
            v_info_value_list := v_info_value_list || format(', %s', testfoundry_random_value(r_field.attname));
        END LOOP;

        EXECUTE format('INSERT INTO %s (%s) VALUES (%s)', v_info_table, v_info_field_list, v_info_value_list);
    END IF;

    -- Start building the field list and value list for the base entity
    v_field_list := format('%I, fk_customer_org, created_by, %I, %I', v_pk_field, v_start_field, v_end_field);
    v_value_list := format('''%s'', ''22222222-2222-2222-2222-222222222222'', ''11111111-1111-1111-1111-111111111111'', ''%s'', ''%s''', v_new_id, v_start_date, v_end_date);

    -- If info table exists, link the fk_<entity>_info field
    IF v_info_exists THEN
        v_field_list := v_field_list || format(', fk_%s_info', p_entity);
        v_value_list := v_value_list || format(', ''%s''', v_info_id);
    END IF;

    -- Find any required parent and link to it
    FOR r_parent IN
        SELECT dependent_entity, link_field
        FROM public.test_entity_dependents
        WHERE dependent_entity = p_entity
    LOOP
        EXECUTE format('SELECT %I FROM %I.%I LIMIT 1',
            r_parent.link_field, p_schema, format('tb_%s', r_parent.parent_entity)
        ) INTO v_parent;

        IF v_parent IS NULL THEN
            RAISE EXCEPTION 'No existing parent entity found for %', r_parent.parent_entity;
        END IF;

        v_field_list := v_field_list || format(', %I', r_parent.link_field);
        v_value_list := v_value_list || format(', ''%s''', v_parent);
    END LOOP;

    -- Handle other fields (non-primary key, non-technical)
    FOR r_field IN
        SELECT attname
        FROM pg_attribute
        WHERE attrelid = format('%I.tb_%s', p_schema, p_entity)::regclass
          AND attnum > 0
          AND NOT attisdropped
          AND attname NOT IN (format('pk_%s', p_entity), 'fk_customer_org', 'created_by', v_start_field, v_end_field, 'fk_' || p_entity || '_info', 'updated_at', 'deleted_at')
    LOOP
        IF NOT EXISTS (
            SELECT 1 FROM public.test_entity_dependents
            WHERE dependent_entity = p_entity AND link_field = r_field.attname
        ) THEN
            v_field_list := v_field_list || format(', %I', r_field.attname);

            IF p_overrides IS NOT NULL AND p_overrides ? r_field.attname THEN
                v_value_list := v_value_list || format(', %s', quote_literal(p_overrides->>r_field.attname));
            ELSE
                v_value_list := v_value_list || format(', %s', testfoundry_random_value(r_field.attname));
            END IF;
        END IF;
    END LOOP;

    -- Insert entity record
    EXECUTE format('INSERT INTO %s (%s) VALUES (%s)', v_table, v_field_list, v_value_list);

    RETURN v_new_id;
END;
$$;
