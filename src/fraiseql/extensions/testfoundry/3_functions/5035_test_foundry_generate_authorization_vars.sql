/**
 * Function: generate_authentication_vars
 * ---------------------------------
 * Generates authentication test variables for organization and user IDs,
 * and prepares mutation_result and UUID placeholders.
 *
 * Returns:
 *   - TEXT block containing variable declarations ready to use with psql's \gset.
 *
 * Usage Example:
 *   SELECT generate_authentication_vars();
 *
 *   -- Output:
 *   SELECT
 *       '22222222-2222-2222-2222-222222222222'::uuid AS v_org,
 *       '11111111-1111-1111-1111-111111111111'::uuid AS v_user,
 *       NULL::mutation_result AS v_result,
 *       NULL::uuid AS v_id
 *   \gset
 */

CREATE OR REPLACE FUNCTION generate_authentication_vars()
RETURNS TEXT
LANGUAGE plpgsql
AS $$
BEGIN
    RETURN
'    SELECT\n'
'        ''22222222-2222-2222-2222-222222222222''::uuid AS v_org,\n'
'        ''11111111-1111-1111-1111-111111111111''::uuid AS v_user,\n'
'        NULL::mutation_result AS v_result,\n'
'        NULL::uuid AS v_id\n'
'    \\gset\n\n';
END;
$$;
