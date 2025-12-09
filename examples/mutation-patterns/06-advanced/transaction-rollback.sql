-- ============================================================================
-- Pattern: Transaction Rollback on Validation
-- ============================================================================
-- Use Case: Validate business rules after partial execution
-- Benefits: Complex validation, rollback on failure, data consistency
--
-- This example shows:
-- - Multi-step operations with validation
-- - Using SAVEPOINT for partial rollback
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
    -- Create Savepoint (in case we need to rollback)
    -- ========================================================================

    SAVEPOINT before_transfer;

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
        ROLLBACK TO SAVEPOINT before_transfer;
        result.status := 'not_found:to_account';
        result.message := 'Destination account not found';
        RETURN result;
    END IF;

    -- ========================================================================
    -- Step 2: Business Rule Validation
    -- ========================================================================

    -- Check sufficient funds
    IF from_account.balance < amount THEN
        ROLLBACK TO SAVEPOINT before_transfer;
        result.status := 'failed:insufficient_funds';
        result.message := format(
            'Insufficient funds. Balance: $%.2f, Required: $%.2f',
            from_account.balance,
            amount
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
        ROLLBACK TO SAVEPOINT before_transfer;
        result.status := 'failed:account_inactive';
        result.message := format('Source account is %s', from_account.status);
        RETURN result;
    END IF;

    IF to_account.status != 'active' THEN
        ROLLBACK TO SAVEPOINT before_transfer;
        result.status := 'failed:account_inactive';
        result.message := format('Destination account is %s', to_account.status);
        RETURN result;
    END IF;

    -- Check daily transfer limit
    DECLARE
        daily_total numeric;
    BEGIN
        SELECT COALESCE(SUM(amount), 0) INTO daily_total
        FROM transfers
        WHERE from_account_id = from_account_id
        AND created_at >= CURRENT_DATE;

        IF (daily_total + amount) > from_account.daily_limit THEN
            ROLLBACK TO SAVEPOINT before_transfer;
            result.status := 'failed:daily_limit_exceeded';
            result.message := format(
                'Daily limit exceeded. Limit: $%.2f, Already transferred: $%.2f',
                from_account.daily_limit,
                daily_total
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

    -- ========================================================================
    -- Success Response
    -- ========================================================================

    RELEASE SAVEPOINT before_transfer;  -- Commit the changes

    result.status := 'updated';
    result.message := format('Transferred $%.2f successfully', amount);
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
