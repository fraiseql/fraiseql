-- Complex domain model with deep nesting for comprehensive benchmarking
-- This schema tests FraiseQL's ability to handle complex object graphs

-- Drop existing schema
DROP SCHEMA IF EXISTS benchmark CASCADE;
CREATE SCHEMA benchmark;
SET search_path TO benchmark, public;

-- Enable extensions
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pg_trgm";

-- Organizations (top level)
CREATE TABLE organizations (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name VARCHAR(255) NOT NULL,
    description TEXT,
    industry VARCHAR(100),
    founded_date DATE,
    headquarters_address JSONB,
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Departments within organizations
CREATE TABLE departments (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    code VARCHAR(50) UNIQUE NOT NULL,
    budget DECIMAL(15, 2),
    head_count INTEGER DEFAULT 0,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Teams within departments
CREATE TABLE teams (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    department_id UUID NOT NULL REFERENCES departments(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    description TEXT,
    formation_date DATE,
    is_active BOOLEAN DEFAULT true,
    performance_metrics JSONB DEFAULT '{}',
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Employees (complex user model)
CREATE TABLE employees (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    email VARCHAR(255) UNIQUE NOT NULL,
    username VARCHAR(100) UNIQUE NOT NULL,
    full_name VARCHAR(255) NOT NULL,
    team_id UUID REFERENCES teams(id) ON DELETE SET NULL,
    role VARCHAR(100),
    level INTEGER CHECK (level BETWEEN 1 AND 10),
    salary DECIMAL(12, 2),
    hire_date DATE NOT NULL,
    skills JSONB DEFAULT '[]',
    certifications JSONB DEFAULT '[]',
    performance_reviews JSONB DEFAULT '[]',
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Projects (complex entities with relationships)
CREATE TABLE projects (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name VARCHAR(255) NOT NULL,
    description TEXT,
    department_id UUID NOT NULL REFERENCES departments(id) ON DELETE CASCADE,
    lead_employee_id UUID REFERENCES employees(id) ON DELETE SET NULL,
    status VARCHAR(50) DEFAULT 'planning',
    priority INTEGER CHECK (priority BETWEEN 1 AND 5),
    budget DECIMAL(15, 2),
    start_date DATE,
    end_date DATE,
    milestones JSONB DEFAULT '[]',
    dependencies JSONB DEFAULT '[]',
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Project team members (many-to-many)
CREATE TABLE project_members (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    employee_id UUID NOT NULL REFERENCES employees(id) ON DELETE CASCADE,
    role VARCHAR(100),
    allocation_percentage INTEGER CHECK (allocation_percentage BETWEEN 0 AND 100),
    start_date DATE NOT NULL,
    end_date DATE,
    UNIQUE(project_id, employee_id)
);

-- Tasks within projects
CREATE TABLE tasks (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    assigned_to_id UUID REFERENCES employees(id) ON DELETE SET NULL,
    title VARCHAR(255) NOT NULL,
    description TEXT,
    status VARCHAR(50) DEFAULT 'todo',
    priority INTEGER CHECK (priority BETWEEN 1 AND 5),
    estimated_hours DECIMAL(6, 2),
    actual_hours DECIMAL(6, 2),
    due_date DATE,
    tags JSONB DEFAULT '[]',
    attachments JSONB DEFAULT '[]',
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Comments on tasks (nested)
CREATE TABLE task_comments (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    task_id UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    author_id UUID NOT NULL REFERENCES employees(id) ON DELETE CASCADE,
    parent_comment_id UUID REFERENCES task_comments(id) ON DELETE CASCADE,
    content TEXT NOT NULL,
    mentions JSONB DEFAULT '[]',
    reactions JSONB DEFAULT '{}',
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Time entries for tasks
CREATE TABLE time_entries (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    task_id UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    employee_id UUID NOT NULL REFERENCES employees(id) ON DELETE CASCADE,
    hours DECIMAL(6, 2) NOT NULL,
    date DATE NOT NULL,
    description TEXT,
    billable BOOLEAN DEFAULT true,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Documents (complex nested structure)
CREATE TABLE documents (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    project_id UUID REFERENCES projects(id) ON DELETE CASCADE,
    author_id UUID NOT NULL REFERENCES employees(id) ON DELETE CASCADE,
    title VARCHAR(255) NOT NULL,
    content TEXT,
    version INTEGER DEFAULT 1,
    status VARCHAR(50) DEFAULT 'draft',
    tags JSONB DEFAULT '[]',
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Document revisions
CREATE TABLE document_revisions (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    author_id UUID NOT NULL REFERENCES employees(id) ON DELETE CASCADE,
    version INTEGER NOT NULL,
    content TEXT NOT NULL,
    change_summary TEXT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Audit log for mutations
CREATE TABLE audit_log (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    entity_type VARCHAR(100) NOT NULL,
    entity_id UUID NOT NULL,
    action VARCHAR(50) NOT NULL,
    actor_id UUID REFERENCES employees(id),
    changes JSONB,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Create indexes for performance
CREATE INDEX idx_departments_org ON departments(organization_id);
CREATE INDEX idx_teams_dept ON teams(department_id);
CREATE INDEX idx_employees_team ON employees(team_id);
CREATE INDEX idx_projects_dept ON projects(department_id);
CREATE INDEX idx_projects_lead ON projects(lead_employee_id);
CREATE INDEX idx_project_members_project ON project_members(project_id);
CREATE INDEX idx_project_members_employee ON project_members(employee_id);
CREATE INDEX idx_tasks_project ON tasks(project_id);
CREATE INDEX idx_tasks_assigned ON tasks(assigned_to_id);
CREATE INDEX idx_task_comments_task ON task_comments(task_id);
CREATE INDEX idx_task_comments_author ON task_comments(author_id);
CREATE INDEX idx_time_entries_task ON time_entries(task_id);
CREATE INDEX idx_time_entries_employee ON time_entries(employee_id);
CREATE INDEX idx_documents_project ON documents(project_id);
CREATE INDEX idx_documents_author ON documents(author_id);
CREATE INDEX idx_document_revisions_document ON document_revisions(document_id);
CREATE INDEX idx_audit_log_entity ON audit_log(entity_type, entity_id);
CREATE INDEX idx_audit_log_actor ON audit_log(actor_id);

-- JSONB indexes
CREATE INDEX idx_employees_skills ON employees USING gin(skills);
CREATE INDEX idx_projects_milestones ON projects USING gin(milestones);
CREATE INDEX idx_tasks_tags ON tasks USING gin(tags);

-- Create complex views for nested queries
CREATE OR REPLACE VIEW v_organization_hierarchy AS
SELECT
    o.id as organization_id,
    o.name as organization_name,
    d.id as department_id,
    d.name as department_name,
    t.id as team_id,
    t.name as team_name,
    e.id as employee_id,
    e.full_name as employee_name,
    e.role as employee_role
FROM organizations o
LEFT JOIN departments d ON d.organization_id = o.id
LEFT JOIN teams t ON t.department_id = d.id
LEFT JOIN employees e ON e.team_id = t.id;

-- Create projection tables for ultra-complex queries
CREATE TABLE tv_organization_full AS
SELECT
    o.id,
    jsonb_build_object(
        'id', o.id::text,
        'name', o.name,
        'description', o.description,
        'industry', o.industry,
        'foundedDate', o.founded_date,
        'headquartersAddress', o.headquarters_address,
        'metadata', o.metadata,
        'createdAt', o.created_at,
        'updatedAt', o.updated_at,
        'departmentCount', (SELECT COUNT(*) FROM departments WHERE organization_id = o.id),
        'employeeCount', (
            SELECT COUNT(*)
            FROM employees e
            JOIN teams t ON e.team_id = t.id
            JOIN departments d ON t.department_id = d.id
            WHERE d.organization_id = o.id
        ),
        'activeProjectCount', (
            SELECT COUNT(*)
            FROM projects p
            JOIN departments d ON p.department_id = d.id
            WHERE d.organization_id = o.id AND p.status != 'completed'
        ),
        'totalBudget', (
            SELECT COALESCE(SUM(budget), 0)
            FROM departments
            WHERE organization_id = o.id
        )
    ) as data
FROM organizations o;

CREATE INDEX idx_tv_organization_full_id ON tv_organization_full(id);

-- Create deeply nested projection for projects
CREATE TABLE tv_project_deep AS
SELECT
    p.id,
    jsonb_build_object(
        'id', p.id::text,
        'name', p.name,
        'description', p.description,
        'status', p.status,
        'priority', p.priority,
        'budget', p.budget,
        'startDate', p.start_date,
        'endDate', p.end_date,
        'milestones', p.milestones,
        'department', jsonb_build_object(
            'id', d.id::text,
            'name', d.name,
            'code', d.code,
            'organization', jsonb_build_object(
                'id', o.id::text,
                'name', o.name,
                'industry', o.industry
            )
        ),
        'leadEmployee', CASE
            WHEN le.id IS NOT NULL THEN jsonb_build_object(
                'id', le.id::text,
                'fullName', le.full_name,
                'email', le.email,
                'role', le.role
            )
            ELSE NULL
        END,
        'taskCount', (SELECT COUNT(*) FROM tasks WHERE project_id = p.id),
        'completedTaskCount', (SELECT COUNT(*) FROM tasks WHERE project_id = p.id AND status = 'completed'),
        'teamSize', (SELECT COUNT(DISTINCT employee_id) FROM project_members WHERE project_id = p.id),
        'totalHoursLogged', (
            SELECT COALESCE(SUM(te.hours), 0)
            FROM time_entries te
            JOIN tasks t ON te.task_id = t.id
            WHERE t.project_id = p.id
        )
    ) as data
FROM projects p
JOIN departments d ON p.department_id = d.id
JOIN organizations o ON d.organization_id = o.id
LEFT JOIN employees le ON p.lead_employee_id = le.id;

CREATE INDEX idx_tv_project_deep_id ON tv_project_deep(id);
CREATE INDEX idx_tv_project_deep_status ON tv_project_deep((data->>'status'));
CREATE INDEX idx_tv_project_deep_priority ON tv_project_deep(((data->>'priority')::int));

-- Insert test data
INSERT INTO organizations (name, description, industry, founded_date, headquarters_address) VALUES
('TechCorp Global', 'Leading technology solutions provider', 'Technology', '2010-01-15', '{"street": "123 Tech Ave", "city": "San Francisco", "state": "CA", "zip": "94105", "country": "USA"}'),
('FinanceHub International', 'Global financial services', 'Finance', '2005-06-20', '{"street": "456 Wall St", "city": "New York", "state": "NY", "zip": "10005", "country": "USA"}'),
('HealthCare Plus', 'Healthcare innovation company', 'Healthcare', '2015-03-10', '{"street": "789 Medical Blvd", "city": "Boston", "state": "MA", "zip": "02110", "country": "USA"}');

-- Insert departments
INSERT INTO departments (organization_id, name, code, budget) VALUES
((SELECT id FROM organizations WHERE name = 'TechCorp Global'), 'Engineering', 'ENG', 5000000),
((SELECT id FROM organizations WHERE name = 'TechCorp Global'), 'Product', 'PROD', 2000000),
((SELECT id FROM organizations WHERE name = 'TechCorp Global'), 'Sales', 'SALES', 3000000),
((SELECT id FROM organizations WHERE name = 'FinanceHub International'), 'Trading', 'TRADE', 10000000),
((SELECT id FROM organizations WHERE name = 'FinanceHub International'), 'Risk Management', 'RISK', 4000000),
((SELECT id FROM organizations WHERE name = 'HealthCare Plus'), 'Research', 'RES', 8000000);

-- Insert teams
INSERT INTO teams (department_id, name, description, formation_date) VALUES
((SELECT id FROM departments WHERE code = 'ENG'), 'Backend Team', 'Core platform development', '2020-01-01'),
((SELECT id FROM departments WHERE code = 'ENG'), 'Frontend Team', 'User interface development', '2020-01-01'),
((SELECT id FROM departments WHERE code = 'ENG'), 'DevOps Team', 'Infrastructure and deployment', '2020-06-01'),
((SELECT id FROM departments WHERE code = 'PROD'), 'Product Strategy', 'Product planning and roadmap', '2019-01-01'),
((SELECT id FROM departments WHERE code = 'SALES'), 'Enterprise Sales', 'Large account management', '2018-01-01');

-- Generate employees with complex data
DO $$
DECLARE
    team_record RECORD;
    i INTEGER;
    skills_array JSONB;
    certs_array JSONB;
BEGIN
    FOR team_record IN SELECT id FROM teams LOOP
        FOR i IN 1..20 LOOP
            skills_array := jsonb_build_array(
                jsonb_build_object('name', 'Python', 'level', 'Expert', 'yearsExperience', 5 + (random() * 10)::int),
                jsonb_build_object('name', 'PostgreSQL', 'level', 'Advanced', 'yearsExperience', 3 + (random() * 7)::int),
                jsonb_build_object('name', 'GraphQL', 'level', 'Intermediate', 'yearsExperience', 1 + (random() * 4)::int)
            );

            certs_array := jsonb_build_array(
                jsonb_build_object('name', 'AWS Certified', 'issueDate', '2023-01-15', 'expiryDate', '2026-01-15'),
                jsonb_build_object('name', 'PostgreSQL Professional', 'issueDate', '2022-06-20', 'expiryDate', '2025-06-20')
            );

            INSERT INTO employees (
                email, username, full_name, team_id, role, level, salary, hire_date, skills, certifications
            ) VALUES (
                'employee' || (team_record.id::text || i::text) || '@company.com',
                'emp_' || substr(md5(random()::text), 1, 8),
                'Employee ' || substr(md5(random()::text), 1, 6),
                team_record.id,
                CASE (random() * 4)::int
                    WHEN 0 THEN 'Junior Developer'
                    WHEN 1 THEN 'Senior Developer'
                    WHEN 2 THEN 'Team Lead'
                    WHEN 3 THEN 'Principal Engineer'
                    ELSE 'Developer'
                END,
                1 + (random() * 9)::int,
                50000 + (random() * 150000)::int,
                CURRENT_DATE - (random() * 1825)::int,
                skills_array,
                certs_array
            );
        END LOOP;
    END LOOP;
END $$;

-- Generate projects with complex relationships
DO $$
DECLARE
    dept_record RECORD;
    emp_record RECORD;
    i INTEGER;
    milestones_array JSONB;
BEGIN
    FOR dept_record IN SELECT id FROM departments LOOP
        FOR i IN 1..10 LOOP
            -- Get a random employee from the department as lead
            SELECT e.id INTO emp_record
            FROM employees e
            JOIN teams t ON e.team_id = t.id
            WHERE t.department_id = dept_record.id
            ORDER BY random()
            LIMIT 1;

            milestones_array := jsonb_build_array(
                jsonb_build_object('name', 'Planning', 'status', 'completed', 'dueDate', '2024-01-15'),
                jsonb_build_object('name', 'Development', 'status', 'in_progress', 'dueDate', '2024-06-15'),
                jsonb_build_object('name', 'Testing', 'status', 'pending', 'dueDate', '2024-08-15'),
                jsonb_build_object('name', 'Deployment', 'status', 'pending', 'dueDate', '2024-09-15')
            );

            INSERT INTO projects (
                name, description, department_id, lead_employee_id,
                status, priority, budget, start_date, end_date, milestones
            ) VALUES (
                'Project ' || substr(md5(random()::text), 1, 8),
                'Description for project focusing on ' || substr(md5(random()::text), 1, 20),
                dept_record.id,
                emp_record.id,
                CASE (random() * 4)::int
                    WHEN 0 THEN 'planning'
                    WHEN 1 THEN 'in_progress'
                    WHEN 2 THEN 'testing'
                    WHEN 3 THEN 'completed'
                    ELSE 'active'
                END,
                1 + (random() * 4)::int,
                100000 + (random() * 900000)::int,
                CURRENT_DATE - (random() * 365)::int,
                CURRENT_DATE + (random() * 365)::int,
                milestones_array
            );
        END LOOP;
    END LOOP;
END $$;

-- Generate project members
INSERT INTO project_members (project_id, employee_id, role, allocation_percentage, start_date)
SELECT
    p.id,
    e.id,
    CASE (random() * 3)::int
        WHEN 0 THEN 'Developer'
        WHEN 1 THEN 'Tester'
        WHEN 2 THEN 'Analyst'
        ELSE 'Contributor'
    END,
    20 + (random() * 80)::int,
    p.start_date + (random() * 30)::int
FROM projects p
CROSS JOIN LATERAL (
    SELECT e.id
    FROM employees e
    JOIN teams t ON e.team_id = t.id
    WHERE t.department_id = p.department_id
    ORDER BY random()
    LIMIT 5 + (random() * 10)::int
) e;

-- Generate tasks for projects
DO $$
DECLARE
    project_record RECORD;
    member_record RECORD;
    i INTEGER;
    tags_array JSONB;
BEGIN
    FOR project_record IN SELECT id FROM projects LOOP
        FOR i IN 1..20 + (random() * 30)::int LOOP
            -- Get a random project member to assign the task
            SELECT employee_id INTO member_record
            FROM project_members
            WHERE project_id = project_record.id
            ORDER BY random()
            LIMIT 1;

            tags_array := jsonb_build_array('backend', 'frontend', 'database', 'api', 'ui', 'performance', 'security', 'testing');

            INSERT INTO tasks (
                project_id, assigned_to_id, title, description,
                status, priority, estimated_hours, due_date, tags
            ) VALUES (
                project_record.id,
                member_record.employee_id,
                'Task: ' || substr(md5(random()::text), 1, 20),
                'Detailed description of the task involving ' || substr(md5(random()::text), 1, 50),
                CASE (random() * 4)::int
                    WHEN 0 THEN 'todo'
                    WHEN 1 THEN 'in_progress'
                    WHEN 2 THEN 'review'
                    WHEN 3 THEN 'completed'
                    ELSE 'todo'
                END,
                1 + (random() * 4)::int,
                4 + (random() * 36)::int,
                CURRENT_DATE + (random() * 90)::int,
                (SELECT jsonb_agg(elem) FROM (
                    SELECT elem FROM jsonb_array_elements(tags_array) elem
                    ORDER BY random()
                    LIMIT 1 + (random() * 3)::int
                ) t)
            );
        END LOOP;
    END LOOP;
END $$;

-- Generate task comments with nesting
DO $$
DECLARE
    task_record RECORD;
    author_record RECORD;
    comment_id UUID;
    i INTEGER;
    j INTEGER;
BEGIN
    FOR task_record IN SELECT id, project_id FROM tasks LIMIT 1000 LOOP
        -- Generate root comments
        FOR i IN 1..2 + (random() * 3)::int LOOP
            SELECT pm.employee_id INTO author_record
            FROM project_members pm
            WHERE pm.project_id = task_record.project_id
            ORDER BY random()
            LIMIT 1;

            INSERT INTO task_comments (
                task_id, author_id, content, mentions
            ) VALUES (
                task_record.id,
                author_record.employee_id,
                'Comment about the task: ' || substr(md5(random()::text), 1, 100),
                jsonb_build_array()
            ) RETURNING id INTO comment_id;

            -- Generate nested replies
            FOR j IN 1..(random() * 2)::int LOOP
                SELECT pm.employee_id INTO author_record
                FROM project_members pm
                WHERE pm.project_id = task_record.project_id
                ORDER BY random()
                LIMIT 1;

                INSERT INTO task_comments (
                    task_id, author_id, parent_comment_id, content
                ) VALUES (
                    task_record.id,
                    author_record.employee_id,
                    comment_id,
                    'Reply to comment: ' || substr(md5(random()::text), 1, 80)
                );
            END LOOP;
        END LOOP;
    END LOOP;
END $$;

-- Generate time entries
INSERT INTO time_entries (task_id, employee_id, hours, date, description, billable)
SELECT
    t.id,
    t.assigned_to_id,
    1 + (random() * 7)::numeric(6,2),
    CURRENT_DATE - (random() * 90)::int,
    'Work on: ' || substr(t.title, 1, 30),
    random() > 0.2
FROM tasks t
WHERE t.assigned_to_id IS NOT NULL
  AND t.status IN ('in_progress', 'completed', 'review');

-- Create mutation functions
CREATE OR REPLACE FUNCTION create_project(
    p_name VARCHAR,
    p_description TEXT,
    p_department_id UUID,
    p_lead_employee_id UUID,
    p_budget DECIMAL,
    p_start_date DATE,
    p_end_date DATE
) RETURNS UUID AS $$
DECLARE
    v_project_id UUID;
BEGIN
    INSERT INTO projects (
        name, description, department_id, lead_employee_id,
        budget, start_date, end_date, status, priority
    ) VALUES (
        p_name, p_description, p_department_id, p_lead_employee_id,
        p_budget, p_start_date, p_end_date, 'planning', 3
    ) RETURNING id INTO v_project_id;

    -- Log the action
    INSERT INTO audit_log (entity_type, entity_id, action, actor_id, changes)
    VALUES ('project', v_project_id, 'create', p_lead_employee_id,
            jsonb_build_object('name', p_name, 'budget', p_budget));

    RETURN v_project_id;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION assign_employee_to_project(
    p_project_id UUID,
    p_employee_id UUID,
    p_role VARCHAR,
    p_allocation INTEGER
) RETURNS UUID AS $$
DECLARE
    v_member_id UUID;
BEGIN
    INSERT INTO project_members (
        project_id, employee_id, role, allocation_percentage, start_date
    ) VALUES (
        p_project_id, p_employee_id, p_role, p_allocation, CURRENT_DATE
    ) RETURNING id INTO v_member_id;

    -- Log the action
    INSERT INTO audit_log (entity_type, entity_id, action, actor_id, changes)
    VALUES ('project_member', v_member_id, 'assign', p_employee_id,
            jsonb_build_object('project_id', p_project_id, 'role', p_role));

    RETURN v_member_id;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION update_task_status(
    p_task_id UUID,
    p_new_status VARCHAR,
    p_actor_id UUID
) RETURNS BOOLEAN AS $$
DECLARE
    v_old_status VARCHAR;
BEGIN
    SELECT status INTO v_old_status FROM tasks WHERE id = p_task_id;

    UPDATE tasks
    SET status = p_new_status,
        updated_at = CURRENT_TIMESTAMP
    WHERE id = p_task_id;

    -- Log the action
    INSERT INTO audit_log (entity_type, entity_id, action, actor_id, changes)
    VALUES ('task', p_task_id, 'status_update', p_actor_id,
            jsonb_build_object('old_status', v_old_status, 'new_status', p_new_status));

    RETURN FOUND;
END;
$$ LANGUAGE plpgsql;

-- Create statistics
ANALYZE;

-- Display summary
SELECT
    'Summary of Complex Test Data:' as info
UNION ALL
SELECT
    'Organizations: ' || COUNT(*)::text FROM organizations
UNION ALL
SELECT
    'Departments: ' || COUNT(*)::text FROM departments
UNION ALL
SELECT
    'Teams: ' || COUNT(*)::text FROM teams
UNION ALL
SELECT
    'Employees: ' || COUNT(*)::text FROM employees
UNION ALL
SELECT
    'Projects: ' || COUNT(*)::text FROM projects
UNION ALL
SELECT
    'Tasks: ' || COUNT(*)::text FROM tasks
UNION ALL
SELECT
    'Task Comments: ' || COUNT(*)::text FROM task_comments
UNION ALL
SELECT
    'Time Entries: ' || COUNT(*)::text FROM time_entries;
