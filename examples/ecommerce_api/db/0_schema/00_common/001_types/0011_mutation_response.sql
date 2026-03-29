CREATE TYPE mutation_response AS (
    status          text,
    message         text,
    entity_id       text,
    entity_type     text,
    entity          jsonb,
    updated_fields  text[],
    cascade         jsonb,
    metadata        jsonb
);
