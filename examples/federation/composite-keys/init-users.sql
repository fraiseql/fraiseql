-- Multi-tenant users database with composite keys (Trinity Pattern)
-- Pattern: tb_* (table), pk_* (INTEGER primary key), id (federation ID), v_* (view)

DROP TABLE IF EXISTS tb_organization_users CASCADE;
DROP TABLE IF EXISTS tb_organizations CASCADE;

CREATE TABLE tb_organizations (
  pk_organization SERIAL PRIMARY KEY,
  id VARCHAR(50) UNIQUE NOT NULL,
  name VARCHAR(255) NOT NULL,
  created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE tb_organization_users (
  pk_org_user SERIAL PRIMARY KEY,
  id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,
  fk_organization INTEGER NOT NULL REFERENCES tb_organizations(pk_organization),
  organization_id VARCHAR(50) NOT NULL,
  user_id VARCHAR(50) NOT NULL,
  name VARCHAR(255) NOT NULL,
  email VARCHAR(255) NOT NULL,
  role VARCHAR(50) NOT NULL,
  created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
  UNIQUE(organization_id, user_id)
);

CREATE INDEX idx_tb_organizations_id ON tb_organizations(id);
CREATE INDEX idx_tb_org_users_fk_organization ON tb_organization_users(fk_organization);
CREATE INDEX idx_tb_org_users_org_id ON tb_organization_users(organization_id);
CREATE INDEX idx_tb_org_users_composite ON tb_organization_users(organization_id, user_id);

-- Create views (Trinity Pattern v_* naming)
CREATE VIEW v_organizations AS
SELECT pk_organization, id, name, created_at
FROM tb_organizations;

CREATE VIEW v_organization_users AS
SELECT pk_org_user, id, fk_organization, organization_id, user_id, name, email, role, created_at
FROM tb_organization_users;

-- Test data: Organization 1
INSERT INTO tb_organizations (id, name) VALUES
  ('org1', 'Acme Corporation'),
  ('org2', 'TechStart Inc');

-- Organization 1 users
INSERT INTO tb_organization_users (fk_organization, organization_id, user_id, name, email, role) VALUES
  (1, 'org1', 'user1', 'Alice Johnson', 'alice@acme.com', 'admin'),
  (1, 'org1', 'user2', 'Bob Smith', 'bob@acme.com', 'member'),
  (1, 'org1', 'user3', 'Charlie Brown', 'charlie@acme.com', 'viewer'),
  (2, 'org2', 'user1', 'Diana Ross', 'diana@techstart.com', 'admin'),
  (2, 'org2', 'user2', 'Eve Wilson', 'eve@techstart.com', 'member');
