CREATE TABLE tb_customer (pk_customer serial PRIMARY KEY, id uuid NOT NULL UNIQUE, tenant_id text NOT NULL, name text NOT NULL, email text NOT NULL);
CREATE TABLE tb_order (pk_order serial PRIMARY KEY, id uuid NOT NULL UNIQUE DEFAULT gen_random_uuid(), tenant_id text NOT NULL, fk_customer uuid NOT NULL REFERENCES tb_customer(id),
  label text NOT NULL, amount numeric NOT NULL, big bigint NOT NULL, created_at timestamptz NOT NULL, status text NOT NULL, secret text NOT NULL, note text, ratio double precision);
INSERT INTO tb_customer (id, tenant_id, name, email)
 SELECT (lpad(to_hex(g),8,'0')||'-0000-4000-8000-000000000000')::uuid, CASE WHEN g%2=0 THEN 'tA' ELSE 'tB' END, 'Cust '||g, 'c'||g||'@x.test' FROM generate_series(1,6) g;
INSERT INTO tb_order (id, tenant_id, fk_customer, label, amount, big, created_at, status, secret, note, ratio)
 SELECT (lpad(to_hex(g),8,'0')||'-1111-4000-8000-000000000000')::uuid,
   CASE WHEN g%3=0 THEN 'tB' ELSE 'tA' END,
   (lpad(to_hex(CASE WHEN g%3=0 THEN 1+2*(g%3) ELSE 2+2*(g%3) END),8,'0')||'-0000-4000-8000-000000000000')::uuid,
   'Order '||g||CASE WHEN g=7 THEN ' ünï "q" \\ ☃' ELSE '' END,
   CASE WHEN g=1 THEN 12345678901234567.890123456789 WHEN g=2 THEN 0.10 WHEN g=3 THEN -1.5 ELSE g*1.25 END,
   CASE WHEN g=1 THEN 9007199254740993 WHEN g=2 THEN -9223372036854775808 ELSE g END,
   timestamptz '2024-03-31 01:30:00+00' + g*interval '37 minutes 1.123456 second',
   (ARRAY['pending','shipped','cancelled'])[1+g%3],
   'SECRET-'||g, CASE WHEN g%4=0 THEN NULL ELSE 'note '||g END,
   CASE WHEN g=1 THEN 0.1+0.2 WHEN g=2 THEN 1e300 WHEN g=3 THEN 5e-324 ELSE g/3.0 END
 FROM generate_series(1,40) g;
CREATE OR REPLACE VIEW v_customer AS SELECT pk_customer, id, tenant_id,
  jsonb_build_object('id',id,'tenant_id',tenant_id,'name',name,'email',email) AS data FROM tb_customer;
CREATE OR REPLACE VIEW v_order AS SELECT o.pk_order, o.id, o.tenant_id, o.fk_customer, o.created_at,
  jsonb_build_object('id',o.id,'tenant_id',o.tenant_id,'fk_customer',o.fk_customer,'label',o.label,'amount',o.amount,'big',o.big,
    'created_at',to_char(o.created_at AT TIME ZONE 'Europe/Paris','YYYY-MM-DD"T"HH24:MI:SS.US')||'+02:00','status',o.status,'secret',o.secret,'note',o.note,'ratio',o.ratio,
    'seen_tenant',current_setting('app.tenant_id',true),'seen_sub',current_setting('app.sub',true),'seen_hdr',current_setting('app.hdr',true),
    'customer',(SELECT c.data FROM v_customer c WHERE c.id=o.fk_customer)) AS data FROM tb_order o;
CREATE OR REPLACE VIEW v_order_rls AS SELECT * FROM v_order WHERE tenant_id = current_setting('app.tenant_id', true);

CREATE OR REPLACE FUNCTION fn_create_order(p_label text, p_amount numeric, p_fk_customer uuid, p_tenant_id text)
RETURNS TABLE(succeeded BOOLEAN, state_changed BOOLEAN, error_class TEXT, status_detail TEXT, http_status SMALLINT, message TEXT, entity_id UUID, entity_type TEXT, entity JSONB, updated_fields TEXT[], cascade JSONB, error_detail JSONB, metadata JSONB)
LANGUAGE plpgsql AS $$
DECLARE v_id uuid;
BEGIN
  INSERT INTO tb_order (tenant_id, fk_customer, label, amount, big, created_at, status, secret, note)
  VALUES (coalesce(p_tenant_id, 'NULL-TENANT'), p_fk_customer, p_label||' sv='||coalesce(current_setting('app.tenant_id',true),'-'), p_amount, 1, now(), 'pending', 'S', NULL) RETURNING tb_order.id INTO v_id;
  RETURN QUERY SELECT * FROM fraiseql.mutation_ok((SELECT data FROM v_order WHERE v_order.id = v_id), v_id, 'Order');
END; $$;
