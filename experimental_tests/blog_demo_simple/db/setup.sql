-- Blog Demo Database Setup Script
-- Loads all database schema and seed data in the correct order

\echo 'Setting up Blog Demo Database...'

-- 1. Common extensions and types
\i db/0_schema/00_common/000_extensions.sql
\i db/0_schema/00_common/001_types.sql

-- 2. Command side tables (write side)
\i db/0_schema/01_write_side/011_users/01101_tb_user.sql
\i db/0_schema/01_write_side/012_content/01201_tb_post.sql
\i db/0_schema/01_write_side/012_content/01202_tb_comment.sql
\i db/0_schema/01_write_side/013_taxonomy/01301_tb_tag.sql
\i db/0_schema/01_write_side/014_associations/01401_tb_post_tag.sql

-- 3. Query side views
\i db/0_schema/02_query_side/021_users/02101_v_user.sql
\i db/0_schema/02_query_side/022_content/02201_v_post.sql
\i db/0_schema/02_query_side/022_content/02202_v_comment.sql
\i db/0_schema/02_query_side/023_taxonomy/02301_v_tag.sql

-- 4. Functions (if any are created)
-- \i db/0_schema/03_functions/030_common/03001_core_utilities.sql

-- 5. Common seed data (always loaded)
\echo 'Loading common seed data...'
\i db/1_seed_data/11_seed_common/11001_seed_users.sql
\i db/1_seed_data/11_seed_common/11002_seed_tags.sql
\i db/1_seed_data/11_seed_common/11003_seed_posts.sql
\i db/1_seed_data/11_seed_common/11004_seed_post_tags.sql
\i db/1_seed_data/11_seed_common/11005_seed_comments.sql

\echo 'Blog Demo Database setup complete!'

-- Optional: Load test-specific seed data
-- \i db/1_seed_data/12_seed_by_test/12001_e2e_workflow_test.sql
