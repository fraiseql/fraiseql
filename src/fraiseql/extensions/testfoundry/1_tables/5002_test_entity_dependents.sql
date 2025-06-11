CREATE TABLE public.testfoundry_tb_entity_dependents (
    id INTEGER GENERATED ALWAYS AS IDENTITY,
    pk_entity_dependent UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    parent_entity TEXT NOT NULL CHECK (parent_entity = lower(parent_entity)),
    dependent_entity TEXT NOT NULL CHECK (dependent_entity = lower(dependent_entity)),
    link_field TEXT NOT NULL, -- example: fk_dns_server, fk_network_configuration
    is_archived_condition TEXT, -- Optional SQL condition to consider a child "archived"
    notes TEXT,

    created_at TIMESTAMPTZ DEFAULT now(),
    updated_at TIMESTAMPTZ DEFAULT now(),

    CONSTRAINT uq_parent_dependent UNIQUE (parent_entity, dependent_entity)
);



-- DNS Server -> Allocation (through network configuration)

INSERT INTO public.testfoundry_tb_entity_dependents (
    parent_entity,
    dependent_entity,
    link_field,
    is_archived_condition,
    notes
) VALUES
(
    'dns_server',
    'allocation',
    'fk_network_configuration',
    'end_date < now()', -- Archived allocations: already ended
    'Allocations must be archived (past end_date) for soft delete.'
);
