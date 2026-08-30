-- Organization Chart LTREE Example — database setup (Trinity Pattern)
-- PostgreSQL
-- Pattern: tb_* (table), pk_* (INTEGER primary key), fk_* (INTEGER foreign key),
--          id (UUID), v_* (view exposing a JSONB `data` column)

CREATE EXTENSION IF NOT EXISTS ltree;

DROP VIEW IF EXISTS v_employee;
DROP TABLE IF EXISTS tb_employee CASCADE;

-- Employee hierarchy table.
--
-- `org_path` is the employee's position in the management chain: every label is
-- an employee, and an employee's parent label is their manager. That is what
-- makes the ltree operators answer organizational questions —
--   <@  descendantOf : everyone under this person, at any depth
--   @>  ancestorOf   : this person's whole management chain
--   nlevel           : their level in the organization
-- A path that interleaved org-unit names with people would still be a valid
-- ltree, but no employee would be another's ancestor and those queries would
-- return nothing useful.
CREATE TABLE tb_employee (
    pk_employee INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID UNIQUE NOT NULL DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    title TEXT,
    department TEXT,
    salary DECIMAL(10,2),
    org_path LTREE NOT NULL UNIQUE,
    fk_manager INT REFERENCES tb_employee(pk_employee),
    hire_date DATE,
    active BOOLEAN DEFAULT true
);

-- GiST index for hierarchical queries (<@, @>, ~)
CREATE INDEX idx_tb_employee_org_path ON tb_employee USING GIST (org_path);
CREATE INDEX idx_tb_employee_id ON tb_employee(id);

-- Sample organization. Fixed UUIDs so the ids in this example's README and
-- queries.graphql stay stable across a rebuild; a real application leaves `id`
-- to its DEFAULT.
INSERT INTO tb_employee (id, name, title, department, salary, org_path, hire_date) VALUES
-- Executive
('33333333-0000-4000-8000-000000000001', 'Alice Johnson', 'CEO', 'Executive', 250000, 'acme.alice_johnson', '2020-01-01'),
('33333333-0000-4000-8000-000000000002', 'Bob Smith', 'CTO', 'Technology', 180000, 'acme.alice_johnson.bob_smith', '2020-02-01'),
('33333333-0000-4000-8000-000000000003', 'Carol Davis', 'CFO', 'Finance', 170000, 'acme.alice_johnson.carol_davis', '2020-03-01'),

-- Engineering, under the CTO
('33333333-0000-4000-8000-000000000004', 'David Wilson', 'VP Engineering', 'Engineering', 150000, 'acme.alice_johnson.bob_smith.david_wilson', '2020-04-01'),
('33333333-0000-4000-8000-000000000005', 'Eva Garcia', 'Backend Manager', 'Engineering', 120000, 'acme.alice_johnson.bob_smith.david_wilson.eva_garcia', '2020-05-01'),
('33333333-0000-4000-8000-000000000006', 'Frank Miller', 'Senior Engineer', 'Engineering', 110000, 'acme.alice_johnson.bob_smith.david_wilson.eva_garcia.frank_miller', '2020-06-01'),
('33333333-0000-4000-8000-000000000007', 'Grace Lee', 'Senior Engineer', 'Engineering', 105000, 'acme.alice_johnson.bob_smith.david_wilson.eva_garcia.grace_lee', '2020-07-01'),
('33333333-0000-4000-8000-000000000008', 'Henry Taylor', 'Junior Engineer', 'Engineering', 85000, 'acme.alice_johnson.bob_smith.david_wilson.eva_garcia.henry_taylor', '2021-01-01'),

-- Frontend team, also under the VP Engineering
('33333333-0000-4000-8000-000000000009', 'Ivy Chen', 'Frontend Manager', 'Engineering', 115000, 'acme.alice_johnson.bob_smith.david_wilson.ivy_chen', '2020-08-01'),
('33333333-0000-4000-8000-00000000000a', 'Jack Brown', 'Senior Frontend Dev', 'Engineering', 100000, 'acme.alice_johnson.bob_smith.david_wilson.ivy_chen.jack_brown', '2020-09-01'),
('33333333-0000-4000-8000-00000000000b', 'Kate White', 'Frontend Developer', 'Engineering', 90000, 'acme.alice_johnson.bob_smith.david_wilson.ivy_chen.kate_white', '2021-02-01'),

-- Product, under the CEO
('33333333-0000-4000-8000-00000000000c', 'Liam Johnson', 'VP Product', 'Product', 145000, 'acme.alice_johnson.liam_johnson', '2020-10-01'),
('33333333-0000-4000-8000-00000000000d', 'Mia Rodriguez', 'Product Manager', 'Product', 110000, 'acme.alice_johnson.liam_johnson.mia_rodriguez', '2020-11-01'),
('33333333-0000-4000-8000-00000000000e', 'Noah Martinez', 'Associate PM', 'Product', 85000, 'acme.alice_johnson.liam_johnson.mia_rodriguez.noah_martinez', '2021-03-01'),

-- Sales, under the CEO
('33333333-0000-4000-8000-00000000000f', 'Olivia Taylor', 'VP Sales', 'Sales', 140000, 'acme.alice_johnson.olivia_taylor', '2020-12-01'),
('33333333-0000-4000-8000-000000000010', 'Parker Wilson', 'Sales Manager', 'Sales', 95000, 'acme.alice_johnson.olivia_taylor.parker_wilson', '2021-01-01'),
('33333333-0000-4000-8000-000000000011', 'Quinn Davis', 'Sales Rep', 'Sales', 75000, 'acme.alice_johnson.olivia_taylor.parker_wilson.quinn_davis', '2021-04-01');

-- Derive fk_manager from the path: an employee's manager is the employee at the
-- parent path. This works because every label in org_path is an employee.
UPDATE tb_employee SET fk_manager = (
    SELECT m.pk_employee FROM tb_employee m
    WHERE m.org_path = subpath(tb_employee.org_path, 0, nlevel(tb_employee.org_path) - 1)
) WHERE nlevel(org_path) > 2;

-- The read side (Trinity Pattern v_* naming). The view returns pk_employee
-- (internal joins), id (native UUID for id lookups) and data (the JSONB payload
-- the runtime reads). `org_path` is a plain string inside `data`; the generated
-- WHERE casts it back with `::ltree` before applying an ltree operator.
CREATE VIEW v_employee AS
SELECT
    e.pk_employee,
    e.id,
    jsonb_build_object(
        'id', e.id,
        'name', e.name,
        'title', e.title,
        'department', e.department,
        'salary', e.salary,
        'org_path', e.org_path::text,
        'hire_date', e.hire_date,
        'active', e.active,
        'manager_name', m.name
    ) AS data
FROM tb_employee e
LEFT JOIN tb_employee m ON e.fk_manager = m.pk_employee;

ANALYZE tb_employee;
