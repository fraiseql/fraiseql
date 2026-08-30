-- ============================================================================
-- Pattern: Transaction Rollback on Validation
-- ============================================================================
-- Use Case: Validate business rules after partial execution
-- Benefits: Complex validation, rollback on failure, data consistency
--
-- This example shows:
-- - Multi-step operations with validation
-- - Using a nested BEGIN ... EXCEPTION block (PL/pgSQL's savepoint) for
--   partial rollback — a function body may not issue SAVEPOINT itself
-- - Rolling back on business rule violations
-- - Detailed error reporting
-- ============================================================================

CREATE OR REPLACE FUNCTION transfer_funds(input_payload jsonb)
RETURNS mutation_response AS $$
DECLARE
    result mutation_response;
    from_account_id uuid := (input_payload->>'from_account_id')::uuid;
    to_account_id uuid := (input_payload->>'to_account_id')::uuid;
    amount numeric := (input_payload->>'amount')::numeric;
    from_account record;
    to_account record;
    new_from_balance numeric;
    new_to_balance numeric;
    transfer_record record;
BEGIN
    -- ========================================================================
    -- Validation
    -- ========================================================================

    IF amount <= 0 THEN
        result.status := 'failed:validation';
        result.message := 'Amount must be positive';
        RETURN result;
    END IF;

    IF from_account_id = to_account_id THEN
        result.status := 'failed:validation';
        result.message := 'Cannot transfer to same account';
        RETURN result;
    END IF;

    -- ========================================================================
    -- Step 1: Lock and Load Accounts
    -- ========================================================================

    SELECT * INTO from_account
    FROM accounts
    WHERE id = from_account_id
    FOR UPDATE;  -- Lock row

    IF NOT FOUND THEN
        result.status := 'not_found:from_account';
        result.message := 'Source account not found';
        RETURN result;
    END IF;

    SELECT * INTO to_account
    FROM accounts
    WHERE id = to_account_id
    FOR UPDATE;  -- Lock row

    IF NOT FOUND THEN
        result.status := 'not_found:to_account';
        result.message := 'Destination account not found';
        RETURN result;
    END IF;

    -- ========================================================================
    -- Step 2: Business Rule Validation
    -- ========================================================================

    -- Check sufficient funds
    IF from_account.balance < amount THEN
        result.status := 'failed:insufficient_funds';
        result.message := format(
            'Insufficient funds. Balance: $%s, Required: $%s',
            to_char(from_account.balance, 'FM999999990.00'),
            to_char(amount, 'FM999999990.00')
        );
        result.metadata := jsonb_build_object(
            'current_balance', from_account.balance,
            'requested_amount', amount,
            'shortfall', amount - from_account.balance
        );
        RETURN result;
    END IF;

    -- Check account status
    IF from_account.status != 'active' THEN
        result.status := 'failed:account_inactive';
        result.message := format('Source account is %s', from_account.status);
        RETURN result;
    END IF;

    IF to_account.status != 'active' THEN
        result.status := 'failed:account_inactive';
        result.message := format('Destination account is %s', to_account.status);
        RETURN result;
    END IF;

    -- Check daily transfer limit
    DECLARE
        daily_total numeric;
        -- Copied into a differently-named local on purpose. The variable and the
        -- column are both `from_account_id`, so `WHERE from_account_id =
        -- from_account_id` compares the column with itself and matches every
        -- row — the daily limit was being computed across all accounts. Naming
        -- the function (`transfer_funds.from_account_id`) does not resolve it
        -- either: PostgreSQL reads that as a table reference and raises
        -- `missing FROM-clause entry for table "transfer_funds"`.
        v_source_account uuid := from_account_id;
    BEGIN
        SELECT COALESCE(SUM(t.amount), 0) INTO daily_total
        FROM transfers t
        WHERE t.from_account_id = v_source_account
        AND t.created_at >= CURRENT_DATE;

        IF (daily_total + amount) > from_account.daily_limit THEN
                result.status := 'failed:daily_limit_exceeded';
            result.message := format(
                'Daily limit exceeded. Limit: $%s, Already transferred: $%s',
                to_char(from_account.daily_limit, 'FM999999990.00'),
                to_char(daily_total, 'FM999999990.00')
            );
            result.metadata := jsonb_build_object(
                'daily_limit', from_account.daily_limit,
                'already_transferred', daily_total,
                'requested_amount', amount,
                'available_today', from_account.daily_limit - daily_total
            );
            RETURN result;
        END IF;
    END;

    -- ========================================================================
    -- Step 3: Perform Transfer
    -- ========================================================================

    -- A nested BEGIN ... EXCEPTION ... END block is PL/pgSQL's savepoint. Entering
    -- it establishes one, and the handler rolls back everything the block did —
    -- which is what this pattern originally reached for with an explicit
    -- SAVEPOINT. A function body may not issue one: PostgreSQL rejected the file
    -- at CREATE FUNCTION with `syntax error at or near "TO"`.
    BEGIN
        -- Debit from source
        UPDATE accounts
        SET balance = balance - amount
        WHERE id = from_account_id
        RETURNING balance INTO new_from_balance;

        -- Credit to destination
        UPDATE accounts
        SET balance = balance + amount
        WHERE id = to_account_id
        RETURNING balance INTO new_to_balance;

        -- Record transfer
        INSERT INTO transfers (from_account_id, to_account_id, amount, status)
        VALUES (from_account_id, to_account_id, amount, 'completed')
        RETURNING * INTO transfer_record;

    EXCEPTION
        WHEN OTHERS THEN
            -- Both UPDATEs and the INSERT are undone; the caller's transaction
            -- survives, which a statement-level failure would not have allowed.
            result.status := 'failed:transfer';
            result.message := format('Transfer could not be completed: %s', SQLERRM);
            RETURN result;
    END;

    -- ========================================================================
    -- Success Response
    -- ========================================================================

    result.status := 'updated';
    result.message := format('Transferred $%s successfully',
                             to_char(amount, 'FM999999990.00'));
    result.entity := row_to_json(transfer_record);
    result.entity_id := transfer_record.id::text;
    result.entity_type := 'Transfer';
    result.metadata := jsonb_build_object(
        'from_account_balance', new_from_balance,
        'to_account_balance', new_to_balance
    );

    RETURN result;

EXCEPTION
    WHEN OTHERS THEN
        -- Automatic rollback on exception
        result.status := 'failed:error';
        result.message := SQLERRM;
        RETURN result;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- Usage Examples
-- ============================================================================

-- Successful transfer
SELECT * FROM transfer_funds('{
    "from_account_id": "550e8400-e29b-41d4-a716-446655440000",
    "to_account_id": "660e8400-e29b-41d4-a716-446655440000",
    "amount": 100.00
}'::jsonb);
-- Returns: status='updated', balances updated atomically

-- Insufficient funds (rolled back)
SELECT * FROM transfer_funds('{
    "from_account_id": "550e8400-e29b-41d4-a716-446655440000",
    "to_account_id": "660e8400-e29b-41d4-a716-446655440000",
    "amount": 999999.00
}'::jsonb);
-- Returns: status='failed:insufficient_funds', NO changes to database

-- Daily limit exceeded (rolled back)
-- Returns: status='failed:daily_limit_exceeded', metadata shows available amount
