-- Migration: rename saga tables from double-prefix to single-prefix (trinity convention)
--
-- Bug: tables were created with the prefix `tb_tb_` (double), violating the
-- project convention of `tb_<entity>` (single prefix).
--
-- This migration is idempotent: each statement uses IF EXISTS / IF NOT EXISTS,
-- so running it multiple times is safe.

-- ── Sequences ─────────────────────────────────────────────────────────────────

ALTER SEQUENCE IF EXISTS seq_tb_tb_federation_sagas
    RENAME TO seq_tb_federation_sagas;

ALTER SEQUENCE IF EXISTS seq_tb_tb_federation_saga_steps
    RENAME TO seq_tb_federation_saga_steps;

ALTER SEQUENCE IF EXISTS seq_tb_tb_federation_saga_recovery
    RENAME TO seq_tb_federation_saga_recovery;

-- ── Indices ───────────────────────────────────────────────────────────────────

ALTER INDEX IF EXISTS idx_tb_tb_federation_sagas_id
    RENAME TO idx_tb_federation_sagas_id;

ALTER INDEX IF EXISTS idx_tb_tb_federation_sagas_state
    RENAME TO idx_tb_federation_sagas_state;

ALTER INDEX IF EXISTS idx_tb_tb_federation_sagas_created
    RENAME TO idx_tb_federation_sagas_created;

ALTER INDEX IF EXISTS idx_tb_tb_federation_saga_steps_id
    RENAME TO idx_tb_federation_saga_steps_id;

ALTER INDEX IF EXISTS idx_tb_tb_federation_saga_steps_saga_pk
    RENAME TO idx_tb_federation_saga_steps_saga_pk;

ALTER INDEX IF EXISTS idx_tb_tb_federation_saga_recovery_id
    RENAME TO idx_tb_federation_saga_recovery_id;

ALTER INDEX IF EXISTS idx_tb_tb_federation_saga_recovery_saga_pk
    RENAME TO idx_tb_federation_saga_recovery_saga_pk;

-- ── Tables (must be last — FK references still hold during rename) ────────────
--
-- Steps and recovery reference the sagas table; rename them first so that the
-- FK constraint still resolves after tb_tb_federation_sagas is renamed.

ALTER TABLE IF EXISTS tb_tb_federation_saga_steps
    RENAME TO tb_federation_saga_steps;

ALTER TABLE IF EXISTS tb_tb_federation_saga_recovery
    RENAME TO tb_federation_saga_recovery;

ALTER TABLE IF EXISTS tb_tb_federation_sagas
    RENAME TO tb_federation_sagas;
