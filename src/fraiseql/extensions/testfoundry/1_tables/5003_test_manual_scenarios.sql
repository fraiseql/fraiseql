-- Create a table for defining manual SQL-based test scenarios
CREATE TABLE public.test_manual_scenarios (
    entity TEXT NOT NULL, -- Name of the entity this test scenario is related to (e.g., 'allocation', 'order')

    scenario TEXT NOT NULL, -- Unique name identifying the scenario within the entity context (e.g., 'overlapping_allocations')

    description TEXT, -- Optional human-readable description explaining the purpose of the test (e.g., 'Check overlap violation for allocations')

    sql_code TEXT NOT NULL, -- The full SQL code block that defines the manual test scenario. It will be injected and executed as part of the test process.

    is_enabled BOOLEAN NOT NULL DEFAULT TRUE, -- Flag to enable/disable a test scenario without physically deleting it.

    created_at TIMESTAMPTZ NOT NULL DEFAULT now() -- Timestamp when the test scenario was created.
);

-- Add table-level comment
COMMENT ON TABLE public.test_manual_scenarios IS
'Table for defining manual SQL-based test scenarios associated with specific entities.
Allows injection of custom test SQL blocks, useful for testing complex behaviors not easily covered by automated generation.';

-- Add column-level comments
COMMENT ON COLUMN public.test_manual_scenarios.entity IS
'Entity name (e.g., "allocation", "order") this manual test scenario targets.';

COMMENT ON COLUMN public.test_manual_scenarios.scenario IS
'Unique scenario name under the given entity, describing the test case (e.g., "overlapping_allocations").';

COMMENT ON COLUMN public.test_manual_scenarios.description IS
'Optional description providing context or intent behind the manual test scenario.';

COMMENT ON COLUMN public.test_manual_scenarios.sql_code IS
'The full SQL code block to be executed for the manual test scenario.';

COMMENT ON COLUMN public.test_manual_scenarios.is_enabled IS
'Boolean flag indicating whether the test scenario is active (TRUE) or disabled (FALSE).';

COMMENT ON COLUMN public.test_manual_scenarios.created_at IS
'Timestamp recording when the test scenario was first created.';


-- example manual scenario

INSERT INTO public.test_manual_scenarios (entity, scenario, description, sql_code)
VALUES (
    'allocation',
    'overlapping_allocations',
    'Ensure that allocations cannot overlap on the same machine (EXCLUDE constraint)',
    $$
    -- Overlapping allocation test
    SELECT plan(3);

    SELECT
        '22222222-2222-2222-2222-222222222222'::uuid AS v_org,
        '11111111-1111-1111-1111-111111111111'::uuid AS v_user,
        NULL::mutation_result AS v_result,
        '6d616368-696e-6500-0000-646e730a0000'::uuid AS v_machine
    \gset

    -- Insert first allocation manually
    INSERT INTO tb_allocation (pk_allocation, fk_machine, start_date, end_date, fk_customer_org)
    VALUES (gen_random_uuid(), :v_machine, '2025-01-01', '2025-01-31', :v_org);

    -- Try inserting overlapping allocation via API
    SELECT * INTO v_result FROM create_allocation_with_log(
        :v_org,
        :v_user,
        jsonb_build_object(
            'machine_id', :v_machine,
            'start_date', '2025-01-15',
            'end_date', '2025-02-15'
        )
    );

    SELECT like(v_result.status, 'error%', 'Should detect overlap error');

    SELECT * FROM finish();
    $$
);
