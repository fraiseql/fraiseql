-- The concurrency oracle's fixture: accounts carry a version counter that every write
-- bumps, and each read shape reads that counter back through a different kind of view.
-- Tenant A owns accounts 1-50, tenant B accounts 51-100.

CREATE TABLE tb_account (
  pk_account serial PRIMARY KEY,
  id uuid NOT NULL UNIQUE,
  tenant_id uuid NOT NULL,
  name text NOT NULL,
  version bigint NOT NULL DEFAULT 0,
  secret text NOT NULL DEFAULT 's'
);
CREATE TABLE tb_item (
  pk_item bigserial PRIMARY KEY,
  id uuid NOT NULL UNIQUE DEFAULT gen_random_uuid(),
  tenant_id uuid NOT NULL,
  fk_account uuid NOT NULL REFERENCES tb_account (id),
  seq bigint NOT NULL,
  label text NOT NULL
);
CREATE INDEX ON tb_item (fk_account);

INSERT INTO tb_account (id, tenant_id, name)
SELECT (lpad(to_hex(g), 8, '0') || '-0000-4000-8000-000000000000')::uuid,
       CASE WHEN g <= 50 THEN '0000000a-0000-4000-8000-000000000000'::uuid
            ELSE '0000000b-0000-4000-8000-000000000000'::uuid END,
       'acct ' || g
FROM generate_series(1, 100) g;
INSERT INTO tb_item (tenant_id, fk_account, seq, label)
SELECT a.tenant_id, a.id, s, 'it ' || s FROM tb_account a, generate_series(1, 3) s;

-- direct, list: the account's own view.
CREATE VIEW v_account AS
SELECT pk_account, id, tenant_id,
       jsonb_build_object('id', id, 'tenant_id', tenant_id, 'name', name, 'version', version,
                          'secret', secret,
                          'seen_tenant', current_setting('app.tenant_id', true)) AS data
FROM tb_account;

-- nested, nestedcol: another entity embeds the account through subqueries, which
-- PostgreSQL records as dependencies of the view.
CREATE VIEW v_item AS
SELECT i.pk_item, i.id, i.tenant_id, i.fk_account, i.seq,
       jsonb_build_object('pk_item', i.pk_item, 'id', i.id, 'tenant_id', i.tenant_id,
                          'fk_account', i.fk_account, 'seq', i.seq, 'label', i.label,
                          'account_version',
                          (SELECT a.version FROM tb_account a WHERE a.id = i.fk_account),
                          'account',
                          (SELECT a.data FROM v_account a WHERE a.id = i.fk_account)) AS data
FROM tb_item i;

-- A plpgsql function's body is opaque to the catalog.
CREATE FUNCTION fn_account_version(p uuid) RETURNS bigint LANGUAGE plpgsql STABLE AS
$$ BEGIN RETURN (SELECT version FROM tb_account WHERE id = p); END $$;

-- A SQL-standard function body: PostgreSQL records its dependency on tb_account.
CREATE FUNCTION fn_account_version_atomic(p uuid) RETURNS bigint LANGUAGE sql STABLE
BEGIN ATOMIC SELECT version FROM tb_account WHERE id = p; END;

-- fn: embeds the account through the plpgsql function, over tb_item, which v_item also
-- reads.
CREATE VIEW v_item_fn AS
SELECT i.pk_item, i.id, i.tenant_id, i.fk_account, i.seq,
       jsonb_build_object('pk_item', i.pk_item, 'id', i.id, 'tenant_id', i.tenant_id,
                          'fk_account', i.fk_account, 'seq', i.seq, 'label', i.label,
                          'account_version', fn_account_version(i.fk_account)) AS data
FROM tb_item i;

-- summary: a second view over the account table.
CREATE VIEW v_account_summary AS
SELECT pk_account, id, tenant_id,
       jsonb_build_object('id', id, 'tenant_id', tenant_id, 'version', version) AS data
FROM tb_account;

-- A view filtered by the session's tenant.
CREATE VIEW v_account_rls AS
SELECT * FROM v_account WHERE tenant_id::text = current_setting('app.tenant_id', true);

-- notefn, noteatomic: tables no other view reads, whose views reach the account only
-- through a function.
CREATE TABLE tb_note (
  pk_note serial PRIMARY KEY,
  id uuid NOT NULL UNIQUE DEFAULT gen_random_uuid(),
  tenant_id uuid NOT NULL,
  fk_account uuid NOT NULL
);
INSERT INTO tb_note (tenant_id, fk_account) SELECT tenant_id, id FROM tb_account;
CREATE VIEW v_note_fn AS
SELECT n.pk_note, n.id, n.tenant_id, n.fk_account,
       jsonb_build_object('id', n.id, 'tenant_id', n.tenant_id, 'fk_account', n.fk_account,
                          'account_version', fn_account_version(n.fk_account)) AS data
FROM tb_note n;

CREATE TABLE tb_note_atomic (
  pk_note serial PRIMARY KEY,
  id uuid NOT NULL UNIQUE DEFAULT gen_random_uuid(),
  tenant_id uuid NOT NULL,
  fk_account uuid NOT NULL
);
INSERT INTO tb_note_atomic (tenant_id, fk_account) SELECT tenant_id, id FROM tb_account;
CREATE VIEW v_note_atomic AS
SELECT n.pk_note, n.id, n.tenant_id, n.fk_account,
       jsonb_build_object('id', n.id, 'tenant_id', n.tenant_id, 'fk_account', n.fk_account,
                          'account_version', fn_account_version_atomic(n.fk_account)) AS data
FROM tb_note_atomic n;

-- Side effects counted by the idempotency cells.
CREATE TABLE tb_effect (
  pk bigserial PRIMARY KEY,
  tag text NOT NULL,
  at timestamptz NOT NULL DEFAULT clock_timestamp()
);

-- Work done at COMMIT: a row inserted here makes the inserting transaction's COMMIT
-- sleep for its `ms`, through a deferred constraint trigger. A client that gives up
-- during that sleep gives up inside COMMIT, every time.
CREATE TABLE tb_commit_delay (pk bigserial PRIMARY KEY, ms integer NOT NULL);
CREATE FUNCTION fn_sleep_at_commit() RETURNS trigger LANGUAGE plpgsql AS
$$ BEGIN PERFORM pg_sleep(NEW.ms / 1000.0); RETURN NULL; END $$;
CREATE CONSTRAINT TRIGGER tr_sleep_at_commit AFTER INSERT ON tb_commit_delay
  DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION fn_sleep_at_commit();

-- slow: account 39's own row, read by a statement that takes 300 ms. Its snapshot is
-- taken when it starts, so a write completing while it runs is not in its answer. The
-- view holds that one row: a filter applies to `data`, so every row in the view would
-- pay the delay.
CREATE FUNCTION fn_slow_read() RETURNS integer LANGUAGE plpgsql VOLATILE AS
$$ BEGIN PERFORM pg_sleep(0.3); RETURN 0; END $$;
CREATE VIEW v_account_slow AS
SELECT pk_account, id, tenant_id,
       jsonb_build_object('id', id, 'tenant_id', tenant_id, 'version', version,
                          'delay', fn_slow_read()) AS data
FROM tb_account
WHERE pk_account = 39;

-- Pagination: account 50 owns 200 items, with repeating `seq` values so the seq orders
-- have ties.
INSERT INTO tb_item (tenant_id, fk_account, seq, label)
SELECT a.tenant_id, a.id, (g * 7) % 50, 'p' || g
FROM tb_account a, generate_series(4, 200) g
WHERE a.pk_account = 50;
