-- PostgreSQL Migration: Create read-side views for CQRS pattern

CREATE VIEW IF NOT EXISTS v_fraise_status AS
SELECT
    fs.pk_fraise_state,
    fs.id,
    fs.identifier,
    fs.fraise_name,
    fs.environment_name,
    fs.job_name,
    fs.current_version,
    fs.status,
    fs.last_deployed_at,
    fs.last_deployed_by,
    (SELECT COUNT(*) FROM tb_deployment d
     WHERE d.fk_fraise_state = fs.pk_fraise_state
       AND d.status = 'success') as successful_deployments,
    (SELECT COUNT(*) FROM tb_deployment d
     WHERE d.fk_fraise_state = fs.pk_fraise_state
       AND d.status = 'failed') as failed_deployments,
    fs.created_at,
    fs.updated_at
FROM tb_fraise_state fs;

CREATE VIEW IF NOT EXISTS v_deployment_history AS
SELECT
    d.pk_deployment,
    d.id,
    d.identifier,
    d.fraise_name,
    d.environment_name,
    d.job_name,
    d.started_at,
    d.completed_at,
    d.duration_seconds,
    d.old_version,
    d.new_version,
    d.status,
    d.triggered_by,
    d.triggered_by_user,
    d.git_commit,
    d.git_branch,
    d.error_message,
    CASE
        WHEN d.old_version != d.new_version THEN 'upgrade'
        WHEN d.old_version = d.new_version THEN 'redeploy'
        ELSE 'unknown'
    END as deployment_type,
    d.created_at,
    d.updated_at
FROM tb_deployment d
ORDER BY d.started_at DESC;

CREATE VIEW IF NOT EXISTS v_webhook_event_history AS
SELECT
    we.pk_webhook_event,
    we.id,
    we.identifier,
    we.git_provider,
    we.event_type,
    we.branch_name,
    we.commit_sha,
    we.sender,
    we.received_at,
    we.processed,
    we.fk_deployment,
    d.id as deployment_id,
    d.fraise_name,
    d.environment_name,
    we.created_at,
    we.updated_at
FROM tb_webhook_event we
LEFT JOIN tb_deployment d ON we.fk_deployment = d.pk_deployment
ORDER BY we.received_at DESC;
