CREATE TABLE public.testfoundry_entity_period_fields (
    id INTEGER GENERATED ALWAYS AS IDENTITY, -- Auto-incrementing integer ID
    pk_entity_period_fields UUID DEFAULT gen_random_uuid() PRIMARY KEY, -- UUID primary key
    entity TEXT NOT NULL,
    start_field TEXT,
    end_field TEXT,
    created_at TIMESTAMPTZ DEFAULT NOW(), -- Timestamp with time zone for record creation
    updated_at TIMESTAMPTZ DEFAULT NOW() -- Timestamp with time zone for record updates
);

-- Optionally, create an index on the 'entity' column if it will be frequently queried
CREATE INDEX idx_entity ON public.testfoundry_entity_period_fields (entity);
