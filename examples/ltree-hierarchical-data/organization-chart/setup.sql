-- Organization Chart LTREE Example
-- Demonstrates employee hierarchy management

CREATE EXTENSION IF NOT EXISTS ltree;

-- Employee hierarchy table
CREATE TABLE tb_employee (
    pk_employee INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID UNIQUE NOT NULL DEFAULT uuid_generate_v4(),
    name TEXT NOT NULL,
    title TEXT,
    department TEXT,
    salary DECIMAL(10,2),
    org_path LTREE NOT NULL,  -- Hierarchical path: company.division.department.manager.employee
    fk_manager INT REFERENCES tb_employee(pk_employee),
    hire_date DATE,
    active BOOLEAN DEFAULT true
);

-- GiST index for hierarchical queries
CREATE INDEX idx_tb_employee_org_path ON tb_employee USING GIST (org_path);
CREATE INDEX idx_tb_employee_id ON tb_employee(id);

-- Sample organization data
INSERT INTO tb_employee (name, title, department, salary, org_path, hire_date) VALUES
-- Executive Level
('Alice Johnson', 'CEO', 'Executive', 250000, 'acme.executive.ceo.alice_johnson', '2020-01-01'),
('Bob Smith', 'CTO', 'Technology', 180000, 'acme.technology.cto.bob_smith', '2020-02-01'),
('Carol Davis', 'CFO', 'Finance', 170000, 'acme.finance.cfo.carol_davis', '2020-03-01'),

-- Engineering Department
('David Wilson', 'VP Engineering', 'Engineering', 150000, 'acme.technology.engineering.vp.david_wilson', '2020-04-01'),
('Eva Garcia', 'Engineering Manager', 'Engineering', 120000, 'acme.technology.engineering.backend.manager.eva_garcia', '2020-05-01'),
('Frank Miller', 'Senior Engineer', 'Engineering', 110000, 'acme.technology.engineering.backend.senior.frank_miller', '2020-06-01'),
('Grace Lee', 'Senior Engineer', 'Engineering', 105000, 'acme.technology.engineering.backend.senior.grace_lee', '2020-07-01'),
('Henry Taylor', 'Junior Engineer', 'Engineering', 85000, 'acme.technology.engineering.backend.junior.henry_taylor', '2021-01-01'),

-- Frontend Team
('Ivy Chen', 'Frontend Manager', 'Engineering', 115000, 'acme.technology.engineering.frontend.manager.ivy_chen', '2020-08-01'),
('Jack Brown', 'Senior Frontend Dev', 'Engineering', 100000, 'acme.technology.engineering.frontend.senior.jack_brown', '2020-09-01'),
('Kate White', 'Frontend Developer', 'Engineering', 90000, 'acme.technology.engineering.frontend.mid.kate_white', '2021-02-01'),

-- Product Department
('Liam Johnson', 'VP Product', 'Product', 145000, 'acme.product.vp.liam_johnson', '2020-10-01'),
('Mia Rodriguez', 'Product Manager', 'Product', 110000, 'acme.product.mobile.manager.mia_rodriguez', '2020-11-01'),
('Noah Martinez', 'Associate PM', 'Product', 85000, 'acme.product.mobile.associate.noah_martinez', '2021-03-01'),

-- Sales Department
('Olivia Taylor', 'VP Sales', 'Sales', 140000, 'acme.sales.vp.olivia_taylor', '2020-12-01'),
('Parker Wilson', 'Sales Manager', 'Sales', 95000, 'acme.sales.enterprise.manager.parker_wilson', '2021-01-01'),
('Quinn Davis', 'Sales Rep', 'Sales', 75000, 'acme.sales.enterprise.rep.quinn_davis', '2021-04-01');

-- Update manager_id references
UPDATE tb_employee SET fk_manager = (
    SELECT pk_employee FROM tb_employee e2 WHERE e2.org_path = subpath(tb_employee.org_path, 0, nlevel(tb_employee.org_path) - 1)
) WHERE nlevel(org_path) > 2;

-- Analyze for query optimization
ANALYZE tb_employee;
