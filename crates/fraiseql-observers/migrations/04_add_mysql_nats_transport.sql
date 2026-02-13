-- FraiseQL Observer System - MySQL NATS Transport Schema
-- This migration adds support for NATS JetStream transport on MySQL:
-- - Transport checkpoint table for cursor persistence
-- - NATS publication tracking columns on entity change log
--
-- Note: MySQL does not have LISTEN/NOTIFY, so the bridge uses polling only.

-- ============================================================================
-- Create Schema (MySQL uses databases, not schemas)
-- ============================================================================
-- Run this in the target database, e.g.: mysql -u root mydb < this_file.sql

-- ============================================================================
-- Transport Checkpoint Table
-- ============================================================================
-- Stores cursor position for event transports, enabling crash recovery.
-- Each transport (e.g., "mysql_to_nats") maintains its own checkpoint.

CREATE TABLE IF NOT EXISTS tb_transport_checkpoint (
    -- Transport identifier (e.g., "mysql_to_nats", "mysql_to_kafka")
    transport_name VARCHAR(255) PRIMARY KEY,

    -- Last processed primary key from source table
    last_pk BIGINT NOT NULL,

    -- When the checkpoint was last updated
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

-- Index for monitoring/debugging queries (find stale checkpoints)
CREATE INDEX idx_transport_checkpoint_updated_at ON tb_transport_checkpoint(updated_at DESC);

-- ============================================================================
-- Entity Change Log Table
-- ============================================================================
-- Stores entity change events for NATS publishing.
-- This table follows the outbox pattern - application writes here,
-- bridge publishes to NATS.

CREATE TABLE IF NOT EXISTS tb_entity_change_log (
    -- Primary key (used as cursor for bridge)
    pk_entity_change_log BIGINT AUTO_INCREMENT PRIMARY KEY,

    -- UUID identifier for the change
    id CHAR(36) NOT NULL,

    -- Customer organization ID (tenant)
    fk_customer_org BIGINT NULL,

    -- Contact ID (user who made the change)
    fk_contact BIGINT NULL,

    -- Entity type (e.g., "Order", "User")
    object_type VARCHAR(255) NOT NULL,

    -- Entity ID (UUID of the changed entity)
    object_id CHAR(36) NOT NULL,

    -- Modification type: INSERT, UPDATE, DELETE
    modification_type VARCHAR(50) NOT NULL,

    -- Change status (e.g., "pending", "processed")
    change_status VARCHAR(50) NULL,

    -- Entity data as JSON
    object_data JSON NULL,

    -- Extra metadata as JSON
    extra_metadata JSON NULL,

    -- When the change was created
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,

    -- When the change was published to NATS (NULL = not published)
    nats_published_at TIMESTAMP NULL,

    -- NATS event ID (for deduplication)
    nats_event_id CHAR(36) NULL,

    -- Indexes
    INDEX idx_entity_change_log_object_type (object_type),
    INDEX idx_entity_change_log_created_at (created_at),
    INDEX idx_entity_change_log_nats_unpublished (pk_entity_change_log) -- For bridge queries
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

-- ============================================================================
-- Add NATS columns if table already exists
-- ============================================================================
-- Use these ALTER statements if tb_entity_change_log already exists

-- ALTER TABLE tb_entity_change_log
--     ADD COLUMN nats_published_at TIMESTAMP NULL,
--     ADD COLUMN nats_event_id CHAR(36) NULL;

-- ============================================================================
-- Helper Stored Procedure: Insert Change Log Entry
-- ============================================================================
-- Call this from application code or triggers to record entity changes.

DELIMITER //

CREATE PROCEDURE IF NOT EXISTS sp_insert_entity_change(
    IN p_object_type VARCHAR(255),
    IN p_object_id CHAR(36),
    IN p_modification_type VARCHAR(50),
    IN p_object_data JSON,
    IN p_fk_customer_org BIGINT,
    IN p_fk_contact BIGINT
)
BEGIN
    INSERT INTO tb_entity_change_log (
        id,
        fk_customer_org,
        fk_contact,
        object_type,
        object_id,
        modification_type,
        object_data
    ) VALUES (
        UUID(),
        p_fk_customer_org,
        p_fk_contact,
        p_object_type,
        p_object_id,
        p_modification_type,
        p_object_data
    );
END //

DELIMITER ;

-- ============================================================================
-- Example Trigger: Auto-capture Order changes
-- ============================================================================
-- Uncomment and modify for your entity tables.

-- DELIMITER //
--
-- CREATE TRIGGER tr_orders_after_insert
-- AFTER INSERT ON orders
-- FOR EACH ROW
-- BEGIN
--     CALL sp_insert_entity_change(
--         'Order',
--         NEW.id,
--         'INSERT',
--         JSON_OBJECT('id', NEW.id, 'total', NEW.total, 'status', NEW.status),
--         NEW.customer_org_id,
--         NEW.created_by
--     );
-- END //
--
-- CREATE TRIGGER tr_orders_after_update
-- AFTER UPDATE ON orders
-- FOR EACH ROW
-- BEGIN
--     CALL sp_insert_entity_change(
--         'Order',
--         NEW.id,
--         'UPDATE',
--         JSON_OBJECT('id', NEW.id, 'total', NEW.total, 'status', NEW.status),
--         NEW.customer_org_id,
--         NEW.updated_by
--     );
-- END //
--
-- CREATE TRIGGER tr_orders_after_delete
-- AFTER DELETE ON orders
-- FOR EACH ROW
-- BEGIN
--     CALL sp_insert_entity_change(
--         'Order',
--         OLD.id,
--         'DELETE',
--         JSON_OBJECT('id', OLD.id),
--         OLD.customer_org_id,
--         NULL
--     );
-- END //
--
-- DELIMITER ;

-- ============================================================================
-- Monitoring View
-- ============================================================================
-- View for monitoring NATS publication status and lag

CREATE OR REPLACE VIEW vw_nats_publication_status AS
SELECT
    checkpoint.transport_name,
    checkpoint.last_pk AS checkpoint_cursor,
    checkpoint.updated_at AS checkpoint_updated_at,
    (SELECT MAX(pk_entity_change_log) FROM tb_entity_change_log) AS max_pk,
    (SELECT MAX(pk_entity_change_log) FROM tb_entity_change_log) - checkpoint.last_pk AS lag_count,
    (SELECT COUNT(*) FROM tb_entity_change_log WHERE nats_published_at IS NULL) AS unpublished_count
FROM tb_transport_checkpoint checkpoint
WHERE checkpoint.transport_name LIKE 'mysql_to_nats%';

-- ============================================================================
-- Comments (MySQL 8.0+ supports table/column comments)
-- ============================================================================

ALTER TABLE tb_transport_checkpoint
    COMMENT = 'Stores cursor position for event transports (NATS bridge, etc.) for crash recovery';

ALTER TABLE tb_entity_change_log
    COMMENT = 'Outbox table for entity change events, published to NATS by bridge process';
