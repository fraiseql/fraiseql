-- FraiseQL-side database (Trinity Pattern).
--
-- This is the *SQL* half of the example: a trivial `User` type so the FraiseQL
-- subgraph has something real to expose alongside the federated async-jobs
-- subgraph. Federation composes the two into one API; this side stays purely
-- SQL-backed, exactly as FraiseQL is designed for.

DROP TABLE IF EXISTS tb_user CASCADE;

CREATE TABLE tb_user (
    pk_user SERIAL PRIMARY KEY,
    id VARCHAR(50) UNIQUE NOT NULL,
    name VARCHAR(255) NOT NULL,
    email VARCHAR(255) NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_tb_user_id ON tb_user(id);

-- Trinity Pattern view: pk_* for internal joins, JSONB `data` for GraphQL.
CREATE VIEW v_user AS
SELECT
    pk_user,
    jsonb_build_object(
        'id', id,
        'name', name,
        'email', email,
        'created_at', created_at
    ) AS data
FROM tb_user;

INSERT INTO tb_user (id, name, email) VALUES
    ('user1', 'Alice Johnson', 'alice@example.com'),
    ('user2', 'Bob Smith', 'bob@example.com');
