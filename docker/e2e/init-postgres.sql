CREATE TABLE IF NOT EXISTS tb_user (
    id   SERIAL PRIMARY KEY,
    name TEXT   NOT NULL
);

INSERT INTO tb_user (name) VALUES ('Alice'), ('Bob'), ('Charlie');

CREATE OR REPLACE VIEW v_users AS
    SELECT id,
           jsonb_build_object('id', id, 'name', name) AS data
    FROM tb_user;
