-- FraiseQL Observer Dispatch Ledger — core.tb_observer_dispatch
-- ============================================================================
-- The durable "already dispatched" record the change-log poller anti-joins
-- against, replacing the strict `pk_entity_change_log > watermark` cursor that
-- permanently skipped late-committing rows (#935).
--
-- Why a ledger and not a watermark
-- --------------------------------
-- `pk_entity_change_log` is `GENERATED ALWAYS AS IDENTITY`: the pk is allocated
-- at INSERT time, inside the writer's transaction, but the row only becomes
-- *visible* at COMMIT. Under concurrent mutations the two orders diverge — tx A
-- takes pk 41 and is still fsyncing while tx B takes pk 42 and commits first. A
-- poll that returns 42 and advances a strict watermark to 42 excludes 41 for
-- ever: its observers never fire, with no error and no trace. This is the same
-- defect family as #797 (the cdc-sinks `MAX(seq)` enqueue cursor), and it takes
-- the same fix: an anti-join against durable per-listener dispatched state, so
-- "have I handled this row?" is answered by a fact rather than by an ordering
-- assumption that does not hold.
--
-- Why the key is the UUID, not the pk
-- -----------------------------------
-- `change_log_id` is `core.tb_entity_change_log.id` — the row's stable public
-- identity, and already the dedup key the delivery contract names (it is
-- `EntityEvent.id` in every dispatched payload). The BIGINT pk is NOT usable
-- here: rebuild the change log and its IDENTITY restarts at 1, so a ledger keyed
-- by pk would match *fresh* rows against a previous incarnation's records and
-- silently suppress their observers — reintroducing, by a different route, the
-- exact silent skip this migration exists to remove. `gen_random_uuid()` never
-- recycles, so a rebuilt log simply has no ledger history, which is the truth.
-- (Compare migration 02, whose checkpoint has to warn operators to delete the
-- cursor row by hand after a rebuild; this table needs no such warning.)
--
-- Columns
-- -------
--   listener_id   — the stable listener identity (the same one keying
--                   `observer_checkpoints`, migration 02). Ledgers are
--                   per-listener: two listeners over one change log each
--                   dispatch every row.
--   change_log_id — the dispatched change-log row's stable UUID.
--   created_at    — a copy of the change-log row's own `created_at`, so retention
--                   can be reasoned about against the same clock the windowed
--                   scan uses.
--   dispatched_at — when the poller recorded the dispatch (observability;
--                   `dispatched_at - created_at` is end-to-end lag).
--
-- The PRIMARY KEY doubles as the anti-join key and as the idempotency guard:
-- recording a dispatch is `ON CONFLICT DO NOTHING`, so a retried record after a
-- crash cannot duplicate.
--
-- Delivery semantics
-- ------------------
-- Still **at-least-once**, unchanged: the ledger row is written *after* the
-- batch's actions were dispatched, so a crash in between re-delivers that batch
-- on restart. What changes is that the replay window stays one batch even though
-- the scan now reaches backwards over a commit-lag window — without the ledger,
-- every restart would re-deliver the whole window. Consumers still dedup on the
-- event id, which is exactly this table's key.
--
-- Retention
-- ---------
-- Prune the ledger **no more aggressively than the change log itself**, and
-- always prune the change log first. A ledger row whose change-log row is gone is
-- inert (the anti-join can never match it), so over-retaining is merely wasteful;
-- under-retaining is a correctness bug — dropping a ledger row while its
-- change-log row survives makes the next full sweep re-dispatch it. The safe
-- statement is therefore:
--
--   DELETE FROM core.tb_observer_dispatch d
--    WHERE NOT EXISTS (SELECT 1 FROM core.tb_entity_change_log e
--                       WHERE e.id = d.change_log_id);
--
-- PostgreSQL only. Idempotent / re-run safe.

CREATE SCHEMA IF NOT EXISTS core;

CREATE TABLE IF NOT EXISTS core.tb_observer_dispatch (
    listener_id   TEXT        NOT NULL,
    change_log_id UUID        NOT NULL,
    created_at    TIMESTAMPTZ,
    dispatched_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (listener_id, change_log_id)
);

-- Retention sweeps and lag monitoring walk the ledger by age, per listener.
CREATE INDEX IF NOT EXISTS idx_observer_dispatch_created
    ON core.tb_observer_dispatch (listener_id, created_at);
