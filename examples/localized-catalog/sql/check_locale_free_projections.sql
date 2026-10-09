-- The projection gate (#1513): no stored projection may depend on the request locale.
--
-- FraiseQL sets `fraiseql.locale` for reads. A view, materialized view, index or trigger
-- function that reads it would store, or serve, one session's language as everyone's —
-- pg_tviews refuses such a definition (#193), and FraiseQL cannot see one at compile time.
-- Run this against your database after your migrations: it lists every object that reads
-- the setting, and should return no rows. Functions named `assert_…` are exempt: a guard
-- that checks the setting is unset is the one legitimate reader.

SELECT 'view' AS kind, schemaname || '.' || viewname AS name
FROM pg_views
WHERE definition ILIKE '%fraiseql.locale%'
  AND schemaname NOT IN ('pg_catalog', 'information_schema')
UNION ALL
SELECT 'materialized view', schemaname || '.' || matviewname
FROM pg_matviews
WHERE definition ILIKE '%fraiseql.locale%'
UNION ALL
SELECT 'index', schemaname || '.' || indexname
FROM pg_indexes
WHERE indexdef ILIKE '%fraiseql.locale%'
UNION ALL
SELECT DISTINCT 'trigger function', n.nspname || '.' || p.proname
FROM pg_trigger tg
JOIN pg_proc p ON p.oid = tg.tgfoid
JOIN pg_namespace n ON n.oid = p.pronamespace
WHERE NOT tg.tgisinternal
  AND p.proname NOT LIKE 'assert\_%'
  AND pg_get_functiondef(p.oid) ILIKE '%fraiseql.locale%'
ORDER BY 1, 2;
