-- Multi-tenant users database with composite keys (Trinity Pattern)
-- Pattern: tb_* (table), pk_* (INTEGER primary key), id (federation ID), v_* (view)

DROP TABLE IF EXISTS tb_organization_user CASCADE;
DROP TABLE IF EXISTS tb_organization CASCADE;

CREATE TABLE tb_organization (
  pk_organization SERIAL PRIMARY KEY,
  id VARCHAR(50) UNIQUE NOT NULL,
  name VARCHAR(255) NOT NULL,
  created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE tb_organization_user (
  pk_org_user SERIAL PRIMARY KEY,
  id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,
  fk_organization INTEGER NOT NULL REFERENCES tb_organization(pk_organization),
  organization_id VARCHAR(50) NOT NULL,
  user_id VARCHAR(50) NOT NULL,
  name VARCHAR(255) NOT NULL,
  email VARCHAR(255) NOT NULL,
  role VARCHAR(50) NOT NULL,
  created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
  UNIQUE(organization_id, user_id)
);

CREATE INDEX idx_tb_organization_id ON tb_organization(id);
CREATE INDEX idx_tb_org_users_fk_organization ON tb_organization_user(fk_organization);
CREATE INDEX idx_tb_org_users_org_id ON tb_organization_user(organization_id);
CREATE INDEX idx_tb_org_users_composite ON tb_organization_user(organization_id, user_id);

-- Create views (Trinity Pattern v_* naming)
-- Returns pk_* (for internal joins) and data (JSONB for GraphQL)
CREATE VIEW v_organization AS
SELECT
    pk_organization,
    jsonb_build_object(
        'id', id,
        'name', name,
        'created_at', created_at
    ) AS data
FROM tb_organization;

CREATE VIEW v_organization_user AS
SELECT
    pk_org_user,
    jsonb_build_object(
        'id', id,
        'organization_id', organization_id,
        'user_id', user_id,
        'name', name,
        'email', email,
        'role', role,
        'created_at', created_at
    ) AS data
FROM tb_organization_user;

-- Test data: Organization 1
INSERT INTO tb_organization (id, name) VALUES
  ('org1', 'Acme Corporation'),
  ('org2', 'TechStart Inc');

-- Organization 1 users
INSERT INTO tb_organization_user (fk_organization, organization_id, user_id, name, email, role) VALUES
  (1, 'org1', 'user1', 'Alice Johnson', 'alice@acme.com', 'admin'),
  (1, 'org1', 'user2', 'Bob Smith', 'bob@acme.com', 'member'),
  (1, 'org1', 'user3', 'Charlie Brown', 'charlie@acme.com', 'viewer'),
  (2, 'org2', 'user1', 'Diana Ross', 'diana@techstart.com', 'admin'),
  (2, 'org2', 'user2', 'Eve Wilson', 'eve@techstart.com', 'member');
