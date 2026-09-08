-- End-to-end fixture: the image-boot, compose-stack, chart-deploy and
-- release-smoke rigs seed a database with this and then assert over it.
--
-- ⚠ Deliberately NOT `public.tb_user`. That name belongs to
-- `tests/sql/postgres/init.sql`, the single owner of the `public` integration
-- fixtures — the premise `crates/fraiseql-db/tests/seed_fixture_integrity.rs`
-- rests on. This file declared it too, with an incompatible shape
-- (`id SERIAL, name TEXT` against `id UUID, data JSONB`), and under
-- `CREATE TABLE IF NOT EXISTS` whichever loaded second was a silent no-op whose
-- dependent objects then failed to build. That is the state #1229 reported and
-- could not attribute, because its grep covered `crates/`, `tests/` and `tools/`
-- and this file is under `docker/` (#1281).
--
-- ⚠ No `IF NOT EXISTS`. Against a database that already carries this table the
-- load must FAIL, here, rather than apply partially and report success —
-- measured against PG16, `IF NOT EXISTS` plus a bare INSERT behaves three ways
-- and two of them are silent. Every consumer had grown its own defence
-- (`DROP SCHEMA`, `ON_ERROR_STOP=1`, a row-count assertion); the loud failure
-- belongs at the collision, not at each of four call sites.
--
-- `v_users` keeps its name: it does not collide with the integration seed's
-- `v_user`, and it is declared as a source in docker/e2e/*.compiled.json.

CREATE TABLE tb_e2e_user (
    id   SERIAL PRIMARY KEY,
    name TEXT   NOT NULL
);

INSERT INTO tb_e2e_user (name) VALUES ('Alice'), ('Bob'), ('Charlie');

CREATE OR REPLACE VIEW v_users AS
    SELECT id,
           jsonb_build_object('id', id, 'name', name) AS data
    FROM tb_e2e_user;
