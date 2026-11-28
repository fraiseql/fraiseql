CREATE TABLE tb_author (pk_author bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, id uuid NOT NULL UNIQUE, data jsonb NOT NULL);
CREATE TABLE tb_post (pk_post bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, id uuid NOT NULL UNIQUE, fk_author bigint REFERENCES tb_author, data jsonb NOT NULL);
INSERT INTO tb_author(id,data) SELECT u, jsonb_build_object('id',u,'name','Author '||g,'email',CASE WHEN g%2=0 THEN 'a'||g||'@x.org' END,'age',CASE WHEN g<>3 THEN 20+g END,'rating',g*1.5,'role',(ARRAY['admin','editor','reader'])[1+g%3],'nicknames',CASE WHEN g%2=1 THEN jsonb_build_array('n'||g,'m'||g) END)
 FROM (SELECT g, (lpad(g::text,8,'0')||'-0000-4000-8000-000000000000')::uuid u FROM generate_series(1,5) g) s;
INSERT INTO tb_post(id,fk_author,data) SELECT u, 1+g%5, jsonb_build_object('id',u,'title','Post '||g,'body',CASE WHEN g%4<>0 THEN 'body '||g END,'views',g*10,'published',g%2=0,'created_at',to_char((timestamptz '2024-01-01'+g*interval '1 day') AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS"Z"'),'tags',jsonb_build_array('t'||g%3))
 FROM (SELECT g,(lpad(g::text,8,'0')||'-1111-4000-8000-000000000000')::uuid u FROM generate_series(1,8) g) s;
CREATE VIEW v_author AS SELECT id, data FROM tb_author;
CREATE VIEW v_post AS SELECT p.id, p.data || jsonb_build_object('author', a.data) AS data FROM tb_post p LEFT JOIN tb_author a ON a.pk_author=p.fk_author;

-- Rows that break the schema's non-null declarations, so error propagation (spec § 6.4.4) is exercised.
INSERT INTO tb_author(id, data) VALUES ('00000006-0000-4000-8000-000000000000',
  '{"id":"00000006-0000-4000-8000-000000000000","name":"NoRole"}');
INSERT INTO tb_post(id, fk_author, data) VALUES ('00000009-1111-4000-8000-000000000000', 6,
  '{"id":"00000009-1111-4000-8000-000000000000","views":90,"published":true,"created_at":"2024-02-01T00:00:00Z","tags":[]}');
