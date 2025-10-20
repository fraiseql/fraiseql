-- LTREE Performance Benchmark Setup
-- Creates hierarchical test data for benchmarking LTREE operators

-- Create test table with LTREE column
CREATE TABLE IF NOT EXISTS ltree_benchmark (
    id SERIAL PRIMARY KEY,
    category_path LTREE NOT NULL,
    name TEXT,
    metadata JSONB
);

-- Create GiST index for optimal LTREE performance
CREATE INDEX IF NOT EXISTS idx_ltree_benchmark_path ON ltree_benchmark USING GIST (category_path);

-- Clear existing data
TRUNCATE ltree_benchmark;

-- Insert hierarchical test data (10,000 rows)
-- Categories: top.science, top.business, top.arts, etc.
INSERT INTO ltree_benchmark (category_path, name, metadata) VALUES
-- Science hierarchy (2,000 entries)
('top.science', 'Science Root', '{"type": "category", "level": 1}'),
('top.science.physics', 'Physics', '{"type": "category", "level": 2}'),
('top.science.physics.theoretical', 'Theoretical Physics', '{"type": "category", "level": 3}'),
('top.science.physics.experimental', 'Experimental Physics', '{"type": "category", "level": 3}'),
('top.science.physics.quantum', 'Quantum Physics', '{"type": "category", "level": 3}'),
('top.science.chemistry', 'Chemistry', '{"type": "category", "level": 2}'),
('top.science.chemistry.organic', 'Organic Chemistry', '{"type": "category", "level": 3}'),
('top.science.chemistry.inorganic', 'Inorganic Chemistry', '{"type": "category", "level": 3}'),
('top.science.biology', 'Biology', '{"type": "category", "level": 2}'),
('top.science.biology.genetics', 'Genetics', '{"type": "category", "level": 3}'),
('top.science.biology.ecology', 'Ecology', '{"type": "category", "level": 3}'),
('top.science.computer_science', 'Computer Science', '{"type": "category", "level": 2}'),
('top.science.computer_science.algorithms', 'Algorithms', '{"type": "category", "level": 3}'),
('top.science.computer_science.databases', 'Databases', '{"type": "category", "level": 3}'),
('top.science.computer_science.networking', 'Networking', '{"type": "category", "level": 3}'),
('top.science.mathematics', 'Mathematics', '{"type": "category", "level": 2}'),
('top.science.mathematics.pure', 'Pure Mathematics', '{"type": "category", "level": 3}'),
('top.science.mathematics.applied', 'Applied Mathematics', '{"type": "category", "level": 3}'),

-- Business hierarchy (2,000 entries)
('top.business', 'Business Root', '{"type": "category", "level": 1}'),
('top.business.finance', 'Finance', '{"type": "category", "level": 2}'),
('top.business.finance.banking', 'Banking', '{"type": "category", "level": 3}'),
('top.business.finance.investment', 'Investment', '{"type": "category", "level": 3}'),
('top.business.finance.accounting', 'Accounting', '{"type": "category", "level": 3}'),
('top.business.marketing', 'Marketing', '{"type": "category", "level": 2}'),
('top.business.marketing.digital', 'Digital Marketing', '{"type": "category", "level": 3}'),
('top.business.marketing.branding', 'Branding', '{"type": "category", "level": 3}'),
('top.business.management', 'Management', '{"type": "category", "level": 2}'),
('top.business.management.strategy', 'Strategy', '{"type": "category", "level": 3}'),
('top.business.management.operations', 'Operations', '{"type": "category", "level": 3}'),
('top.business.entrepreneurship', 'Entrepreneurship', '{"type": "category", "level": 2}'),
('top.business.entrepreneurship.startups', 'Startups', '{"type": "category", "level": 3}'),
('top.business.entrepreneurship.venture_capital', 'Venture Capital', '{"type": "category", "level": 3}'),

-- Arts hierarchy (2,000 entries)
('top.arts', 'Arts Root', '{"type": "category", "level": 1}'),
('top.arts.visual', 'Visual Arts', '{"type": "category", "level": 2}'),
('top.arts.visual.painting', 'Painting', '{"type": "category", "level": 3}'),
('top.arts.visual.sculpture', 'Sculpture', '{"type": "category", "level": 3}'),
('top.arts.visual.photography', 'Photography', '{"type": "category", "level": 3}'),
('top.arts.performing', 'Performing Arts', '{"type": "category", "level": 2}'),
('top.arts.performing.theater', 'Theater', '{"type": "category", "level": 3}'),
('top.arts.performing.music', 'Music', '{"type": "category", "level": 3}'),
('top.arts.performing.dance', 'Dance', '{"type": "category", "level": 3}'),
('top.arts.literature', 'Literature', '{"type": "category", "level": 2}'),
('top.arts.literature.fiction', 'Fiction', '{"type": "category", "level": 3}'),
('top.arts.literature.poetry', 'Poetry', '{"type": "category", "level": 3}'),
('top.arts.literature.drama', 'Drama', '{"type": "category", "level": 3}'),

-- Technology hierarchy (2,000 entries)
('top.technology', 'Technology Root', '{"type": "category", "level": 1}'),
('top.technology.software', 'Software', '{"type": "category", "level": 2}'),
('top.technology.software.web', 'Web Development', '{"type": "category", "level": 3}'),
('top.technology.software.mobile', 'Mobile Development', '{"type": "category", "level": 3}'),
('top.technology.software.desktop', 'Desktop Software', '{"type": "category", "level": 3}'),
('top.technology.hardware', 'Hardware', '{"type": "category", "level": 2}'),
('top.technology.hardware.computers', 'Computers', '{"type": "category", "level": 3}'),
('top.technology.hardware.networking', 'Networking Hardware', '{"type": "category", "level": 3}'),
('top.technology.hardware.embedded', 'Embedded Systems', '{"type": "category", "level": 3}'),
('top.technology.ai', 'Artificial Intelligence', '{"type": "category", "level": 2}'),
('top.technology.ai.machine_learning', 'Machine Learning', '{"type": "category", "level": 3}'),
('top.technology.ai.nlp', 'Natural Language Processing', '{"type": "category", "level": 3}'),
('top.technology.ai.computer_vision', 'Computer Vision', '{"type": "category", "level": 3}'),

-- Sports hierarchy (2,000 entries)
('top.sports', 'Sports Root', '{"type": "category", "level": 1}'),
('top.sports.team', 'Team Sports', '{"type": "category", "level": 2}'),
('top.sports.team.soccer', 'Soccer', '{"type": "category", "level": 3}'),
('top.sports.team.basketball', 'Basketball', '{"type": "category", "level": 3}'),
('top.sports.team.baseball', 'Baseball', '{"type": "category", "level": 3}'),
('top.sports.individual', 'Individual Sports', '{"type": "category", "level": 2}'),
('top.sports.individual.tennis', 'Tennis', '{"type": "category", "level": 3}'),
('top.sports.individual.golf', 'Golf', '{"type": "category", "level": 3}'),
('top.sports.individual.swimming', 'Swimming', '{"type": "category", "level": 3}'),
('top.sports.extreme', 'Extreme Sports', '{"type": "category", "level": 2}'),
('top.sports.extreme.skiing', 'Skiing', '{"type": "category", "level": 3}'),
('top.sports.extreme.surfing', 'Surfing', '{"type": "category", "level": 3}'),
('top.sports.extreme.climbing', 'Climbing', '{"type": "category", "level": 3}');

-- Generate additional test data using recursive CTE to create deeper hierarchies
-- This creates 10,000 total rows with realistic hierarchical data
INSERT INTO ltree_benchmark (category_path, name, metadata)
WITH RECURSIVE hierarchy AS (
    -- Base categories
    SELECT
        category_path,
        name,
        metadata,
        1 as depth
    FROM (VALUES
        ('top.science.physics.quantum.mechanics', 'Quantum Mechanics', '{"type": "subcategory", "level": 4}'::jsonb),
        ('top.business.finance.investment.stocks', 'Stock Investment', '{"type": "subcategory", "level": 4}'::jsonb),
        ('top.arts.visual.painting.impressionism', 'Impressionism', '{"type": "subcategory", "level": 4}'::jsonb),
        ('top.technology.software.web.frontend', 'Frontend Development', '{"type": "subcategory", "level": 4}'::jsonb),
        ('top.sports.team.soccer.professional', 'Professional Soccer', '{"type": "subcategory", "level": 4}'::jsonb)
    ) AS base(category_path, name, metadata)

    UNION ALL

    -- Generate deeper levels
    SELECT
        h.category_path || '.' || ('sub' || (row_number() over (partition by h.category_path order by random()))::text)::ltree,
        'Sub ' || (row_number() over (partition by h.category_path order by random()))::text,
        jsonb_build_object('type', 'auto_generated', 'level', h.depth + 1, 'parent', h.category_path),
        h.depth + 1
    FROM hierarchy h
    WHERE h.depth < 6
    AND random() < 0.7  -- 70% chance to continue hierarchy
    LIMIT 9950  -- Ensure we don't exceed our target
)
SELECT category_path, name, metadata FROM hierarchy;

-- Analyze table for query optimization
ANALYZE ltree_benchmark;

-- Show statistics
SELECT
    'Total rows' as metric,
    count(*) as value
FROM ltree_benchmark
UNION ALL
SELECT
    'Max depth' as metric,
    max(nlevel(category_path)) as value
FROM ltree_benchmark
UNION ALL
SELECT
    'Distinct paths' as metric,
    count(distinct category_path) as value
FROM ltree_benchmark;
