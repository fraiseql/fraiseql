-- FraiseQL localized catalog example (#1512, #1513).
--
-- `tv_product.data->'name'` is a locale map. FraiseQL reads it through the request locale's
-- fallback chain; nothing stored here depends on who reads or writes it.

CREATE SCHEMA IF NOT EXISTS app;
DO $$ BEGIN
    CREATE TYPE app.mutation_error_class AS ENUM ('validation', 'conflict', 'not_found',
        'unauthorized', 'forbidden', 'internal', 'transaction_failed', 'timeout',
        'rate_limited', 'service_unavailable');
EXCEPTION WHEN duplicate_object THEN NULL; END $$;
DO $$ BEGIN
    CREATE TYPE app.mutation_response AS (succeeded BOOLEAN, state_changed BOOLEAN,
        error_class app.mutation_error_class, status_detail TEXT, http_status SMALLINT,
        message TEXT, entity_id UUID, entity_type TEXT, entity JSONB, updated_fields TEXT[],
        cascade JSONB, error_detail JSONB, metadata JSONB);
EXCEPTION WHEN duplicate_object THEN NULL; END $$;

-- A tenant, and the locale its people read in. The server's `[identity.enrichment]` reads
-- it for a caller (`tenant_locale`), and the order view below renders labels in it.
CREATE TABLE tb_tenant (
    id     uuid PRIMARY KEY,
    sub    text NOT NULL UNIQUE,
    locale text NOT NULL
);

-- The shared catalog. `data->'name'` = {"fr-FR": "Pomme", "en-US": "Apple", …}.
CREATE TABLE tv_product (
    pk   bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id   uuid NOT NULL UNIQUE,
    data jsonb NOT NULL
);

CREATE TABLE tb_order (
    id         uuid PRIMARY KEY,
    fk_tenant  uuid NOT NULL REFERENCES tb_tenant (id),
    fk_product uuid NOT NULL REFERENCES tv_product (id),
    quantity   int NOT NULL
);

-- Tenant-scoped data rendered in the tenant's STORED locale: a plain join on a stored fact,
-- so the view, and anything materialized from it, is the same whoever reads it. A view
-- reading `current_setting('fraiseql.locale')` would not be (pg_tviews refuses one, #193).
CREATE VIEW v_tenant_order AS
SELECT o.id,
       jsonb_build_object(
           'id', o.id,
           'tenant_id', o.fk_tenant,
           'quantity', o.quantity,
           'product_label', COALESCE(p.data->'name'->>t.locale, p.data->'name'->>'en-US')
       ) AS data
FROM tb_order o
JOIN tb_tenant t ON t.id = o.fk_tenant
JOIN tv_product p ON p.id = o.fk_product;

-- Set some of a product's labels and keep the others; a null label removes its key.
-- FraiseQL hands `p_name` over as a map whatever shape the client wrote.
CREATE FUNCTION fn_rename_product(p_id uuid, p_name jsonb)
RETURNS app.mutation_response LANGUAGE plpgsql AS $$
DECLARE result app.mutation_response;
BEGIN
    UPDATE tv_product
       SET data = jsonb_set(data, '{name}', jsonb_strip_nulls((data->'name') || p_name))
     WHERE id = p_id;
    result.succeeded := true;
    result.state_changed := true;
    result.message := 'renamed';
    result.entity_type := 'Product';
    result.entity_id := p_id;
    result.entity := (SELECT data FROM tv_product WHERE id = p_id);
    RETURN result;
END $$;

-- The write-session guard: FraiseQL sets `fraiseql.locale` on reads only, so a write — and
-- any projection refreshed inside it — never sees a locale. This trigger refuses a write
-- that does. It is the one function allowed to read the setting (see
-- check_locale_free_projections.sql). Unset reads as NULL on a fresh connection and as ''
-- on a pooled one that set it in an earlier transaction: both mean "no locale".
CREATE FUNCTION assert_write_has_no_locale() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF coalesce(current_setting('fraiseql.locale', true), '') <> '' THEN
        RAISE EXCEPTION 'a write ran with fraiseql.locale = %',
            current_setting('fraiseql.locale', true);
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER write_has_no_locale BEFORE INSERT OR UPDATE ON tv_product
    FOR EACH ROW EXECUTE FUNCTION assert_write_has_no_locale();

-- `fraiseql compile` reports one expression index per allowed locale for `Product.name`;
-- `fraiseql doctor --against-db` names any that is missing.

-- Every signed-in caller belongs to a tenant: the enrichment is fail-closed, so a subject
-- with no row is refused. Visitors share a guest tenant whose stored locale is the default.
INSERT INTO tb_tenant VALUES
    ('00000000-0000-0000-0000-0000000000a1', 'alice', 'de-DE'),
    ('00000000-0000-0000-0000-0000000000b2', 'bob', 'fr-FR'),
    ('00000000-0000-0000-0000-0000000000c3', 'visitor', 'en-US');

INSERT INTO tv_product (id, data) VALUES
    ('00000000-0000-0000-0000-000000000001', '{"id": "00000000-0000-0000-0000-000000000001",
      "sku": "APL", "name": {"en-US": "Apple", "fr-FR": "Pomme", "de-DE": "Apfel"}}'),
    ('00000000-0000-0000-0000-000000000002', '{"id": "00000000-0000-0000-0000-000000000002",
      "sku": "PER", "name": {"en-US": "Pear", "fr": "Poire", "de-DE": "Birne"}}'),
    ('00000000-0000-0000-0000-000000000003', '{"id": "00000000-0000-0000-0000-000000000003",
      "sku": "CHR", "name": {"en-US": "Cherry", "fr-FR": "Cerise", "de-DE": "Kirsche"}}');

INSERT INTO tb_order VALUES
    ('00000000-0000-0000-0000-00000000a001', '00000000-0000-0000-0000-0000000000a1',
     '00000000-0000-0000-0000-000000000001', 3),
    ('00000000-0000-0000-0000-00000000b001', '00000000-0000-0000-0000-0000000000b2',
     '00000000-0000-0000-0000-000000000003', 5);
