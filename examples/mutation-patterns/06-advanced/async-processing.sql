-- ============================================================================
-- Pattern: Async Job Processing
-- ============================================================================
-- Use Case: Queue long-running operations for background processing
-- Benefits: Fast response, scalability, retry capability, progress tracking
--
-- This example shows:
-- - Queueing jobs instead of blocking
-- - Returning job ID for status tracking
-- - Job status enum pattern
-- - NOTIFY for real-time updates
-- ============================================================================

CREATE TYPE job_status AS ENUM ('pending', 'processing', 'completed', 'failed');

CREATE OR REPLACE FUNCTION import_users_async(input_payload jsonb)
RETURNS mutation_response AS $$
DECLARE
    result mutation_response;
    job_record record;
    csv_data text := input_payload->>'csv_data';
    user_count int;
BEGIN
    -- ========================================================================
    -- Validation
    -- ========================================================================

    IF csv_data IS NULL OR csv_data = '' THEN
        result.status := 'failed:validation';
        result.message := 'CSV data is required';
        RETURN result;
    END IF;

    -- Quick validation: count lines
    user_count := array_length(string_to_array(csv_data, E'\n'), 1) - 1;  -- Minus header

    IF user_count <= 0 THEN
        result.status := 'failed:validation';
        result.message := 'CSV must contain at least one user';
        RETURN result;
    END IF;

    -- ========================================================================
    -- Create Job (Don't Process Yet)
    -- ========================================================================

    INSERT INTO jobs (
        type,
        status,
        input_data,
        metadata
    ) VALUES (
        'import_users',
        'pending',
        input_payload,
        jsonb_build_object(
            'user_count', user_count,
            'submitted_at', now()
        )
    )
    RETURNING * INTO job_record;

    -- ========================================================================
    -- Notify Background Worker
    -- ========================================================================

    PERFORM pg_notify('job_queue', json_build_object(
        'job_id', job_record.id,
        'type', 'import_users'
    )::text);

    -- ========================================================================
    -- Success Response (Job Queued)
    -- ========================================================================

    result.status := 'created';
    result.message := format('Import job queued. %s user(s) to process', user_count);
    result.entity := row_to_json(job_record);
    result.entity_id := job_record.id::text;
    result.entity_type := 'Job';
    result.metadata := jsonb_build_object(
        'job_id', job_record.id,
        'status', 'pending',
        'estimated_duration_seconds', user_count * 0.1,  -- ~0.1s per user
        'check_status_url', format('/jobs/%s', job_record.id)
    );

    RETURN result;

EXCEPTION
    WHEN OTHERS THEN
        result.status := 'failed:error';
        result.message := SQLERRM;
        RETURN result;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- Job Status Check Function
-- ============================================================================

CREATE OR REPLACE FUNCTION get_job_status(input_payload jsonb)
RETURNS mutation_response AS $$
DECLARE
    result mutation_response;
    job_record record;
    job_id uuid := (input_payload->>'job_id')::uuid;
BEGIN
    SELECT * INTO job_record FROM jobs WHERE id = job_id;

    IF NOT FOUND THEN
        result.status := 'not_found:job';
        result.message := 'Job not found';
        RETURN result;
    END IF;

    result.status := 'success';
    result.message := format('Job status: %s', job_record.status);
    result.entity := row_to_json(job_record);
    result.entity_id := job_record.id::text;
    result.entity_type := 'Job';

    RETURN result;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- Background Worker Function (Simulated)
-- ============================================================================

CREATE OR REPLACE FUNCTION process_import_job(job_id uuid)
RETURNS void AS $$
DECLARE
    job_record record;
    csv_data text;
    csv_line text;
    success_count int := 0;
    error_count int := 0;
    errors jsonb := '[]'::jsonb;
BEGIN
    -- Mark job as processing
    UPDATE jobs SET status = 'processing', started_at = now()
    WHERE id = job_id
    RETURNING * INTO job_record;

    csv_data := job_record.input_data->>'csv_data';

    -- Process each line (simplified)
    FOR csv_line IN
        SELECT unnest(string_to_array(csv_data, E'\n'))
        OFFSET 1  -- Skip header
    LOOP
        BEGIN
            -- Parse and insert user
            DECLARE
                fields text[] := string_to_array(csv_line, ',');
            BEGIN
                INSERT INTO users (email, name)
                VALUES (fields[1], fields[2]);
                success_count := success_count + 1;
            END;
        EXCEPTION
            WHEN OTHERS THEN
                error_count := error_count + 1;
                errors := errors || jsonb_build_object(
                    'line', csv_line,
                    'error', SQLERRM
                );
        END;
    END LOOP;

    -- Mark job as completed
    UPDATE jobs SET
        status = CASE WHEN error_count = 0 THEN 'completed' ELSE 'failed' END,
        completed_at = now(),
        result_data = jsonb_build_object(
            'success_count', success_count,
            'error_count', error_count,
            'errors', errors
        )
    WHERE id = job_id;

    -- Notify completion
    PERFORM pg_notify('job_completed', json_build_object(
        'job_id', job_id,
        'status', CASE WHEN error_count = 0 THEN 'completed' ELSE 'failed' END,
        'success_count', success_count,
        'error_count', error_count
    )::text);
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- Usage Examples
-- ============================================================================

-- Submit import job
SELECT * FROM import_users_async('{
    "csv_data": "email,name\njohn@example.com,John Doe\njane@example.com,Jane Smith"
}'::jsonb);
-- Returns: status='created', entity.id=job_id, metadata.check_status_url

-- Check job status
SELECT * FROM get_job_status('{
    "job_id": "770e8400-e29b-41d4-a716-446655440000"
}'::jsonb);
-- Returns: status='success', entity.status='processing|completed|failed'

-- ============================================================================
-- GraphQL Subscription Example
-- ============================================================================

/*
mutation ImportUsers {
  importUsersAsync(input: {
    csvData: "email,name\n..."
  }) {
    ... on ImportJobQueued {
      job {
        id
        status
        metadata
      }
    }
  }
}

subscription JobStatus($jobId: UUID!) {
  jobUpdated(jobId: $jobId) {
    id
    status
    resultData
    completedAt
  }
}
*/

-- ============================================================================
-- Pattern Benefits
-- ============================================================================

/*
1. Fast Response: Mutation returns immediately, doesn't block
2. Scalability: Background workers can process jobs in parallel
3. Retry Logic: Failed jobs can be retried automatically
4. Progress Tracking: Job status can be polled or subscribed to
5. User Experience: User gets feedback and can continue working
6. Resource Management: Rate limit background processing

Common Async Patterns:
- Bulk imports (CSV, Excel)
- Report generation (PDF, Excel exports)
- Email sending (newsletters, notifications)
- Image processing (resize, optimize)
- Data migrations
- External API calls
*/
