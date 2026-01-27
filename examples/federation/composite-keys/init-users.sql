-- Multi-tenant users database with composite keys

CREATE TABLE organizations (
  id VARCHAR(50) PRIMARY KEY,
  name VARCHAR(255) NOT NULL,
  created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE organization_users (
  organization_id VARCHAR(50) NOT NULL,
  user_id VARCHAR(50) NOT NULL,
  name VARCHAR(255) NOT NULL,
  email VARCHAR(255) NOT NULL,
  role VARCHAR(50) NOT NULL,
  created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
  PRIMARY KEY (organization_id, user_id),
  FOREIGN KEY (organization_id) REFERENCES organizations(id)
);

CREATE INDEX idx_org_id ON organization_users(organization_id);
CREATE INDEX idx_org_user_id ON organization_users(organization_id, user_id);

-- Test data: Organization 1
INSERT INTO organizations (id, name) VALUES
  ('org1', 'Acme Corporation'),
  ('org2', 'TechStart Inc');

-- Organization 1 users
INSERT INTO organization_users (organization_id, user_id, name, email, role) VALUES
  ('org1', 'user1', 'Alice Johnson', 'alice@acme.com', 'admin'),
  ('org1', 'user2', 'Bob Smith', 'bob@acme.com', 'member'),
  ('org1', 'user3', 'Charlie Brown', 'charlie@acme.com', 'viewer'),
  ('org2', 'user1', 'Diana Ross', 'diana@techstart.com', 'admin'),
  ('org2', 'user2', 'Eve Wilson', 'eve@techstart.com', 'member');
