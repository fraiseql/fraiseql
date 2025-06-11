-- ============================================================================
-- Function: testfoundry_random_latitude
-- Purpose:
--   Generate a random latitude value between -90 and 90 degrees.
-- ============================================================================
CREATE OR REPLACE FUNCTION testfoundry_random_latitude()
RETURNS FLOAT
LANGUAGE SQL
AS $$
SELECT round((random() * 180.0 - 90.0)::NUMERIC, 6);
$$;


-- ============================================================================
-- Function: testfoundry_random_longitude
-- Purpose:
--   Generate a random longitude value between -180 and 180 degrees.
-- ============================================================================
CREATE OR REPLACE FUNCTION testfoundry_random_longitude()
RETURNS FLOAT
LANGUAGE SQL
AS $$
SELECT round((random() * 360.0 - 180.0)::NUMERIC, 6);
$$;
