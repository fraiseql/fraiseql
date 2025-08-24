-- Core utility functions for mutation handling
-- Following the printoptim_backend patterns

-- Function to sanitize JSONB input (remove unset values)
CREATE OR REPLACE FUNCTION core.sanitize_jsonb_unset(input_data JSONB)
RETURNS JSONB AS $$
BEGIN
    -- Remove keys with null values that represent "unset" fields
    RETURN (
        SELECT jsonb_object_agg(key, value)
        FROM jsonb_each(input_data)
        WHERE value IS NOT NULL AND value != 'null'::jsonb
    );
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Function to log and return mutation results
CREATE OR REPLACE FUNCTION core.log_and_return_mutation(
    success BOOLEAN,
    message TEXT,
    object_data JSONB DEFAULT NULL,
    error_code TEXT DEFAULT NULL,
    metadata JSONB DEFAULT NULL
) RETURNS mutation_result AS $$
DECLARE
    result mutation_result;
BEGIN
    -- Build the result
    result.success := success;
    result.message := message;
    result.object_data := object_data;
    result.error_code := error_code;
    result.metadata := COALESCE(metadata, '{}'::jsonb);

    -- TODO: Add logging to mutation log table if needed

    RETURN result;
END;
$$ LANGUAGE plpgsql;

-- Function to generate unique slugs
CREATE OR REPLACE FUNCTION core.generate_unique_slug(
    base_slug TEXT,
    table_name TEXT,
    exclude_id UUID DEFAULT NULL
) RETURNS TEXT AS $$
DECLARE
    counter INTEGER := 0;
    new_slug TEXT := base_slug;
    exists_count INTEGER;
    query_text TEXT;
BEGIN
    LOOP
        -- Build dynamic query to check if slug exists
        IF exclude_id IS NOT NULL THEN
            query_text := format('SELECT COUNT(*) FROM %I WHERE identifier = $1 AND pk_%s != $2',
                               table_name,
                               substring(table_name from 4)); -- Remove 'tb_' prefix
            EXECUTE query_text USING new_slug, exclude_id INTO exists_count;
        ELSE
            query_text := format('SELECT COUNT(*) FROM %I WHERE identifier = $1', table_name);
            EXECUTE query_text USING new_slug INTO exists_count;
        END IF;

        -- If slug doesn't exist, return it
        IF exists_count = 0 THEN
            RETURN new_slug;
        END IF;

        -- Increment counter and try again
        counter := counter + 1;
        new_slug := base_slug || '-' || counter;
    END LOOP;
END;
$$ LANGUAGE plpgsql;

-- Function to create slug from text
CREATE OR REPLACE FUNCTION core.slugify(text_input TEXT)
RETURNS TEXT AS $$
BEGIN
    RETURN lower(
        regexp_replace(
            regexp_replace(
                unaccent(text_input),
                '[^a-zA-Z0-9\s-]', '', 'g'
            ),
            '\s+', '-', 'g'
        )
    );
END;
$$ LANGUAGE plpgsql IMMUTABLE;
