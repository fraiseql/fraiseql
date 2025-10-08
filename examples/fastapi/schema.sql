-- FastAPI Example Database Schema
-- Task Management System

-- Enable extensions
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- Users table
CREATE TABLE tb_users (
    id SERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    email VARCHAR(255) UNIQUE NOT NULL,
    avatar_url VARCHAR(500),
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Projects table
CREATE TABLE tb_projects (
    id SERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    description TEXT,
    owner_id INT NOT NULL REFERENCES tb_users(id) ON DELETE CASCADE,
    status VARCHAR(50) NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'archived', 'completed')),
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Tasks table
CREATE TABLE tb_tasks (
    id SERIAL PRIMARY KEY,
    project_id INT NOT NULL REFERENCES tb_projects(id) ON DELETE CASCADE,
    title VARCHAR(500) NOT NULL,
    description TEXT,
    status VARCHAR(50) NOT NULL DEFAULT 'todo' CHECK (status IN ('todo', 'in_progress', 'completed', 'blocked')),
    priority VARCHAR(50) NOT NULL DEFAULT 'medium' CHECK (priority IN ('low', 'medium', 'high', 'urgent')),
    assignee_id INT REFERENCES tb_users(id) ON DELETE SET NULL,
    due_date TIMESTAMP,
    completed_at TIMESTAMP,
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Indexes for performance
CREATE INDEX idx_projects_owner ON tb_projects(owner_id);
CREATE INDEX idx_projects_status ON tb_projects(status) WHERE status != 'archived';
CREATE INDEX idx_tasks_project ON tb_tasks(project_id);
CREATE INDEX idx_tasks_assignee ON tb_tasks(assignee_id);
CREATE INDEX idx_tasks_status ON tb_tasks(status);
CREATE INDEX idx_tasks_priority ON tb_tasks(priority) WHERE priority IN ('high', 'urgent');
CREATE INDEX idx_tasks_due_date ON tb_tasks(due_date) WHERE due_date IS NOT NULL AND status != 'completed';

-- Composite indexes for common queries
CREATE INDEX idx_tasks_project_status ON tb_tasks(project_id, status);
CREATE INDEX idx_tasks_assignee_status ON tb_tasks(assignee_id, status) WHERE assignee_id IS NOT NULL;

-- Views for GraphQL queries

CREATE VIEW v_users AS
SELECT
    id,
    name,
    email,
    avatar_url,
    created_at,
    updated_at
FROM tb_users;

CREATE VIEW v_projects AS
SELECT
    p.id,
    p.name,
    p.description,
    p.owner_id,
    p.status,
    p.created_at,
    p.updated_at,
    u.name as owner_name,
    (SELECT COUNT(*) FROM tb_tasks WHERE project_id = p.id) as task_count,
    (SELECT COUNT(*) FROM tb_tasks WHERE project_id = p.id AND status = 'completed') as completed_count
FROM tb_projects p
LEFT JOIN tb_users u ON p.owner_id = u.id;

CREATE VIEW v_tasks AS
SELECT
    t.id,
    t.project_id,
    t.title,
    t.description,
    t.status,
    t.priority,
    t.assignee_id,
    t.due_date,
    t.completed_at,
    t.created_at,
    t.updated_at,
    p.name as project_name,
    u.name as assignee_name
FROM tb_tasks t
LEFT JOIN tb_projects p ON t.project_id = p.id
LEFT JOIN tb_users u ON t.assignee_id = u.id;

-- PostgreSQL Functions for Mutations

-- Create a new project
CREATE OR REPLACE FUNCTION fn_create_project(
    p_name VARCHAR(255),
    p_description TEXT,
    p_owner_id INT
)
RETURNS TABLE(
    id INT,
    name VARCHAR(255),
    description TEXT,
    owner_id INT,
    status VARCHAR(50),
    created_at TIMESTAMP,
    updated_at TIMESTAMP
) AS $$
BEGIN
    RETURN QUERY
    INSERT INTO tb_projects (name, description, owner_id)
    VALUES (p_name, p_description, p_owner_id)
    RETURNING
        tb_projects.id,
        tb_projects.name,
        tb_projects.description,
        tb_projects.owner_id,
        tb_projects.status,
        tb_projects.created_at,
        tb_projects.updated_at;
END;
$$ LANGUAGE plpgsql;

-- Update a project
CREATE OR REPLACE FUNCTION fn_update_project(
    p_id INT,
    p_name VARCHAR(255) DEFAULT NULL,
    p_description TEXT DEFAULT NULL,
    p_status VARCHAR(50) DEFAULT NULL
)
RETURNS TABLE(
    id INT,
    name VARCHAR(255),
    description TEXT,
    owner_id INT,
    status VARCHAR(50),
    created_at TIMESTAMP,
    updated_at TIMESTAMP
) AS $$
BEGIN
    RETURN QUERY
    UPDATE tb_projects
    SET
        name = COALESCE(p_name, tb_projects.name),
        description = COALESCE(p_description, tb_projects.description),
        status = COALESCE(p_status, tb_projects.status),
        updated_at = NOW()
    WHERE tb_projects.id = p_id
    RETURNING
        tb_projects.id,
        tb_projects.name,
        tb_projects.description,
        tb_projects.owner_id,
        tb_projects.status,
        tb_projects.created_at,
        tb_projects.updated_at;
END;
$$ LANGUAGE plpgsql;

-- Create a new task
CREATE OR REPLACE FUNCTION fn_create_task(
    p_project_id INT,
    p_title VARCHAR(500),
    p_description TEXT DEFAULT NULL,
    p_priority VARCHAR(50) DEFAULT 'medium',
    p_status VARCHAR(50) DEFAULT 'todo',
    p_assignee_id INT DEFAULT NULL,
    p_due_date TIMESTAMP DEFAULT NULL
)
RETURNS TABLE(
    id INT,
    project_id INT,
    title VARCHAR(500),
    description TEXT,
    status VARCHAR(50),
    priority VARCHAR(50),
    assignee_id INT,
    due_date TIMESTAMP,
    completed_at TIMESTAMP,
    created_at TIMESTAMP,
    updated_at TIMESTAMP
) AS $$
BEGIN
    RETURN QUERY
    INSERT INTO tb_tasks (project_id, title, description, priority, status, assignee_id, due_date)
    VALUES (p_project_id, p_title, p_description, p_priority, p_status, p_assignee_id, p_due_date)
    RETURNING
        tb_tasks.id,
        tb_tasks.project_id,
        tb_tasks.title,
        tb_tasks.description,
        tb_tasks.status,
        tb_tasks.priority,
        tb_tasks.assignee_id,
        tb_tasks.due_date,
        tb_tasks.completed_at,
        tb_tasks.created_at,
        tb_tasks.updated_at;
END;
$$ LANGUAGE plpgsql;

-- Update a task
CREATE OR REPLACE FUNCTION fn_update_task(
    p_id INT,
    p_title VARCHAR(500) DEFAULT NULL,
    p_description TEXT DEFAULT NULL,
    p_status VARCHAR(50) DEFAULT NULL,
    p_priority VARCHAR(50) DEFAULT NULL,
    p_assignee_id INT DEFAULT NULL,
    p_due_date TIMESTAMP DEFAULT NULL
)
RETURNS TABLE(
    id INT,
    project_id INT,
    title VARCHAR(500),
    description TEXT,
    status VARCHAR(50),
    priority VARCHAR(50),
    assignee_id INT,
    due_date TIMESTAMP,
    completed_at TIMESTAMP,
    created_at TIMESTAMP,
    updated_at TIMESTAMP
) AS $$
DECLARE
    new_status VARCHAR(50);
    old_status VARCHAR(50);
BEGIN
    -- Get current status
    SELECT tb_tasks.status INTO old_status FROM tb_tasks WHERE tb_tasks.id = p_id;
    new_status := COALESCE(p_status, old_status);

    RETURN QUERY
    UPDATE tb_tasks
    SET
        title = COALESCE(p_title, tb_tasks.title),
        description = COALESCE(p_description, tb_tasks.description),
        status = new_status,
        priority = COALESCE(p_priority, tb_tasks.priority),
        assignee_id = CASE
            WHEN p_assignee_id IS NULL AND p_assignee_id IS NOT DISTINCT FROM NULL THEN tb_tasks.assignee_id
            ELSE p_assignee_id
        END,
        due_date = CASE
            WHEN p_due_date IS NULL AND p_due_date IS NOT DISTINCT FROM NULL THEN tb_tasks.due_date
            ELSE p_due_date
        END,
        -- Auto-set completed_at when status changes to completed
        completed_at = CASE
            WHEN new_status = 'completed' AND old_status != 'completed' THEN NOW()
            WHEN new_status != 'completed' THEN NULL
            ELSE tb_tasks.completed_at
        END,
        updated_at = NOW()
    WHERE tb_tasks.id = p_id
    RETURNING
        tb_tasks.id,
        tb_tasks.project_id,
        tb_tasks.title,
        tb_tasks.description,
        tb_tasks.status,
        tb_tasks.priority,
        tb_tasks.assignee_id,
        tb_tasks.due_date,
        tb_tasks.completed_at,
        tb_tasks.created_at,
        tb_tasks.updated_at;
END;
$$ LANGUAGE plpgsql;

-- Assign a task to a user
CREATE OR REPLACE FUNCTION fn_assign_task(
    p_task_id INT,
    p_user_id INT
)
RETURNS TABLE(
    id INT,
    project_id INT,
    title VARCHAR(500),
    description TEXT,
    status VARCHAR(50),
    priority VARCHAR(50),
    assignee_id INT,
    due_date TIMESTAMP,
    completed_at TIMESTAMP,
    created_at TIMESTAMP,
    updated_at TIMESTAMP
) AS $$
BEGIN
    RETURN QUERY
    UPDATE tb_tasks
    SET
        assignee_id = p_user_id,
        updated_at = NOW()
    WHERE tb_tasks.id = p_task_id
    RETURNING
        tb_tasks.id,
        tb_tasks.project_id,
        tb_tasks.title,
        tb_tasks.description,
        tb_tasks.status,
        tb_tasks.priority,
        tb_tasks.assignee_id,
        tb_tasks.due_date,
        tb_tasks.completed_at,
        tb_tasks.created_at,
        tb_tasks.updated_at;
END;
$$ LANGUAGE plpgsql;

-- Delete a task (soft delete would be better in production)
CREATE OR REPLACE FUNCTION fn_delete_task(p_id INT)
RETURNS BOOLEAN AS $$
BEGIN
    DELETE FROM tb_tasks WHERE id = p_id;
    RETURN FOUND;
END;
$$ LANGUAGE plpgsql;

-- Sample data
INSERT INTO tb_users (name, email, avatar_url) VALUES
('Alice Johnson', 'alice@example.com', 'https://i.pravatar.cc/150?img=1'),
('Bob Smith', 'bob@example.com', 'https://i.pravatar.cc/150?img=2'),
('Carol Williams', 'carol@example.com', 'https://i.pravatar.cc/150?img=3'),
('David Brown', 'david@example.com', 'https://i.pravatar.cc/150?img=4');

INSERT INTO tb_projects (name, description, owner_id, status) VALUES
('FraiseQL Core', 'Core GraphQL framework development', 1, 'active'),
('TurboRouter', 'High-performance query optimization', 2, 'active'),
('Documentation', 'Improve docs and examples', 1, 'active'),
('Marketing Website', 'Build fraiseql.dev landing page', 3, 'completed');

INSERT INTO tb_tasks (project_id, title, description, status, priority, assignee_id, due_date) VALUES
(1, 'Implement JSON passthrough', 'Zero-copy JSON handling for better performance', 'completed', 'high', 1, NOW() - INTERVAL '5 days'),
(1, 'Add support for IPv6 types', 'Support PostgreSQL INET/CIDR types', 'in_progress', 'medium', 2, NOW() + INTERVAL '7 days'),
(1, 'Write comprehensive tests', 'Achieve 90% code coverage', 'todo', 'medium', NULL, NOW() + INTERVAL '14 days'),
(2, 'Design TurboQuery registry', 'Hash-based lookup system', 'completed', 'urgent', 2, NOW() - INTERVAL '10 days'),
(2, 'Implement APQ integration', 'Automatic Persisted Queries support', 'in_progress', 'high', 2, NOW() + INTERVAL '3 days'),
(2, 'Benchmark against alternatives', 'Compare with Hasura, PostGraphile', 'todo', 'low', 3, NOW() + INTERVAL '21 days'),
(3, 'Create FastAPI example', 'Complete working example', 'completed', 'medium', 4, NOW()),
(3, 'Fix markdown formatting', 'ReadTheDocs compatibility', 'completed', 'high', 1, NOW() - INTERVAL '1 day'),
(3, 'Add CQRS patterns guide', 'Enterprise architecture examples', 'todo', 'medium', 1, NOW() + INTERVAL '5 days'),
(4, 'Design landing page', 'Modern, fast, professional', 'completed', 'high', 3, NOW() - INTERVAL '30 days'),
(4, 'Write marketing copy', 'Clear value propositions', 'completed', 'high', 3, NOW() - INTERVAL '25 days'),
(4, 'Launch on Product Hunt', 'Community outreach', 'completed', 'medium', 3, NOW() - INTERVAL '20 days');

-- Update completed_at for completed tasks
UPDATE tb_tasks SET completed_at = created_at + INTERVAL '2 days' WHERE status = 'completed';
