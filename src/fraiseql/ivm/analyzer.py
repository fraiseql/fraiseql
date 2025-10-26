"""IVM analyzer for detecting optimal tv_ table update strategies.

Analyzes denormalized JSONB tables (tv_*) to determine which should use
incremental updates via jsonb_merge_shallow vs full rebuilds.

Uses EXPLICIT SYNC pattern (not triggers) for industrial control:
- Mutation functions explicitly call sync_tv_table()
- Full visibility into when sync happens
- Easy to test, debug, and optimize
"""

import logging
from dataclasses import dataclass
from typing import Any

from psycopg_pool import AsyncConnectionPool

logger = logging.getLogger(__name__)


@dataclass
class IVMCandidate:
    """Represents a tv_ table candidate for incremental maintenance.

    Attributes:
        table_name: Name of the tv_ table (e.g., "tv_user", "tv_post")
        source_table: Corresponding tb_ source table (e.g., "tb_user")
        row_count: Number of rows in the tv_ table
        avg_jsonb_size: Average size of JSONB data column in bytes
        jsonb_field_count: Average number of top-level fields in JSONB
        update_frequency: Estimated updates per minute
        complexity_score: Overall complexity score (0.0-10.0)
        recommendation: "incremental" or "full_rebuild"
        confidence: Confidence level in recommendation (0.0-1.0)
    """

    table_name: str
    source_table: str | None
    row_count: int
    avg_jsonb_size: int
    jsonb_field_count: int
    update_frequency: float
    complexity_score: float
    recommendation: str
    confidence: float

    def __str__(self) -> str:
        return f"{self.table_name}: {self.recommendation} (score: {self.complexity_score:.1f})"


@dataclass
class IVMRecommendation:
    """Overall IVM setup recommendation with specific actions.

    Attributes:
        total_tv_tables: Total number of tv_ tables found
        incremental_candidates: List of tables recommended for incremental updates
        full_rebuild_candidates: List of tables that should keep full rebuilds
        estimated_speedup: Estimated overall speedup factor
        setup_sql: SQL to set up universal sync system
        sync_helpers: Python helper functions for explicit sync
        mutation_examples: Example mutation functions with explicit sync
    """

    total_tv_tables: int
    incremental_candidates: list[IVMCandidate]
    full_rebuild_candidates: list[IVMCandidate]
    estimated_speedup: float
    setup_sql: str
    sync_helpers: str
    mutation_examples: str

    def __str__(self) -> str:
        return (
            f"IVM Analysis: {len(self.incremental_candidates)}/{self.total_tv_tables} "
            f"tables benefit from incremental updates (est. {self.estimated_speedup:.1f}x speedup)"
        )


class IVMAnalyzer:
    """Analyzes tv_ tables to recommend optimal update strategies.

    This analyzer examines denormalized JSONB tables (tv_*) and determines
    which should use incremental updates with jsonb_merge_shallow() versus
    full rebuilds.

    Decision Factors:
        - Table size (rows): Larger tables benefit more from incremental
        - JSONB complexity: More fields = more benefit from partial updates
        - Update frequency: Frequent updates favor incremental approach
        - Update pattern: Partial field updates vs full rewrites

    Example:
        ```python
        analyzer = IVMAnalyzer(connection_pool)
        recommendation = await analyzer.analyze()

        print(recommendation)
        # IVM Analysis: 5/8 tables benefit from incremental updates (est. 25.3x speedup)

        # Apply recommendations
        await analyzer.setup_incremental_triggers(recommendation.incremental_candidates)
        ```
    """

    def __init__(
        self,
        connection_pool: AsyncConnectionPool,
        *,
        min_rows_threshold: int = 1000,
        min_jsonb_fields: int = 5,
        incremental_score_threshold: float = 5.0,
    ) -> None:
        """Initialize IVM analyzer.

        Args:
            connection_pool: psycopg connection pool
            min_rows_threshold: Minimum rows to consider incremental (default: 1000)
            min_jsonb_fields: Minimum JSONB fields to benefit (default: 5)
            incremental_score_threshold: Score threshold for incremental (default: 5.0)
        """
        self.pool = connection_pool
        self.min_rows_threshold = min_rows_threshold
        self.min_jsonb_fields = min_jsonb_fields
        self.incremental_score_threshold = incremental_score_threshold

        self.has_jsonb_ivm: bool = False
        self.extension_version: str | None = None

    async def check_extension(self) -> bool:
        """Check if jsonb_ivm extension is installed.

        Returns:
            True if extension is available, False otherwise
        """
        try:
            async with self.pool.connection() as conn, conn.cursor() as cur:
                await cur.execute(
                    """
                    SELECT extversion
                    FROM pg_extension
                    WHERE extname = 'jsonb_ivm'
                """
                )
                result = await cur.fetchone()

                if result:
                    self.has_jsonb_ivm = True
                    self.extension_version = result[0] if result[0] is not None else None
                    logger.info("✓ Detected jsonb_ivm v%s", self.extension_version)
                    return True

                logger.warning("jsonb_ivm extension not installed")
                return False

        except Exception as e:
            logger.error("Failed to check jsonb_ivm extension: %s", e)
            return False

    async def discover_tv_tables(self) -> list[str]:
        """Discover all tv_ tables in the database.

        Returns:
            List of tv_ table names
        """
        try:
            async with self.pool.connection() as conn, conn.cursor() as cur:
                await cur.execute(
                    """
                    SELECT tablename
                    FROM pg_tables
                    WHERE schemaname NOT IN ('pg_catalog', 'information_schema')
                      AND tablename LIKE 'tv_%'
                    ORDER BY tablename
                """
                )

                rows = await cur.fetchall()
                tables = [row[0] for row in rows]

                logger.info("Discovered %d tv_ tables", len(tables))
                return tables

        except Exception as e:
            logger.error("Failed to discover tv_ tables: %s", e)
            return []

    async def analyze_table(self, table_name: str) -> IVMCandidate | None:
        """Analyze a single tv_ table for IVM candidacy.

        Args:
            table_name: Name of the tv_ table to analyze

        Returns:
            IVMCandidate with analysis results, or None if analysis failed
        """
        try:
            async with self.pool.connection() as conn, conn.cursor() as cur:
                # Get row count
                await cur.execute(f"SELECT COUNT(*) FROM {table_name}")
                result = await cur.fetchone()
                row_count = result[0] if result else 0

                # Analyze JSONB structure (assuming 'data' column contains JSONB)
                await cur.execute(
                    f"""
                    SELECT
                        AVG(pg_column_size(data))::INT as avg_size,
                        AVG((SELECT COUNT(*) FROM jsonb_object_keys(data)))::INT as avg_fields
                    FROM {table_name}
                    WHERE data IS NOT NULL
                    LIMIT 1000
                    """,
                )

                result = await cur.fetchone()
                if not result or result[0] is None:
                    # No data column or no data
                    logger.debug("Table %s has no JSONB data to analyze", table_name)
                    return None

                avg_jsonb_size = int(result[0] or 0)
                jsonb_field_count = int(result[1] or 0)

                # Infer source table (tv_user → tb_user)
                source_table = table_name.replace("tv_", "tb_", 1)

                # Check if source table exists
                await cur.execute(
                    """
                    SELECT EXISTS (
                        SELECT 1 FROM pg_tables
                        WHERE tablename = %s
                    )
                    """,
                    (source_table,),
                )
                result = await cur.fetchone()
                source_exists = result[0] if result else False

                if not source_exists:
                    source_table = None

                # Estimate update frequency (from statistics if available)
                # For now, using a simple heuristic
                update_frequency = 0.0  # Updates per minute (unknown)

                # Calculate complexity score
                complexity_score = self._calculate_complexity_score(
                    row_count=row_count,
                    jsonb_size=avg_jsonb_size,
                    field_count=jsonb_field_count,
                    update_freq=update_frequency,
                )

                # Make recommendation
                recommendation = (
                    "incremental"
                    if complexity_score >= self.incremental_score_threshold
                    else "full_rebuild"
                )

                # Calculate confidence
                confidence = min(1.0, complexity_score / 10.0)

                candidate = IVMCandidate(
                    table_name=table_name,
                    source_table=source_table,
                    row_count=row_count,
                    avg_jsonb_size=avg_jsonb_size,
                    jsonb_field_count=jsonb_field_count,
                    update_frequency=update_frequency,
                    complexity_score=complexity_score,
                    recommendation=recommendation,
                    confidence=confidence,
                )

                logger.debug("Analyzed %s: %s", table_name, candidate)
                return candidate

        except Exception as e:
            logger.error("Failed to analyze table %s: %s", table_name, e)
            return None

    def _calculate_complexity_score(
        self,
        row_count: int,
        jsonb_size: int,
        field_count: int,
        update_freq: float,
    ) -> float:
        """Calculate complexity score for IVM recommendation.

        Higher score = more benefit from incremental updates.

        Args:
            row_count: Number of rows in table
            jsonb_size: Average JSONB size in bytes
            field_count: Average number of JSONB fields
            update_freq: Updates per minute

        Returns:
            Complexity score (0.0-10.0)
        """
        score = 0.0

        # Factor 1: Table size (0-3 points)
        if row_count > 100_000:
            score += 3.0
        elif row_count > 10_000:
            score += 2.0
        elif row_count > 1_000:
            score += 1.0

        # Factor 2: JSONB field count (0-3 points)
        if field_count > 20:
            score += 3.0
        elif field_count > 10:
            score += 2.0
        elif field_count > 5:
            score += 1.0

        # Factor 3: JSONB size (0-2 points)
        if jsonb_size > 10_000:  # > 10KB
            score += 2.0
        elif jsonb_size > 2_000:  # > 2KB
            score += 1.0

        # Factor 4: Update frequency (0-2 points)
        if update_freq > 10:  # > 10 updates/min
            score += 2.0
        elif update_freq > 1:  # > 1 update/min
            score += 1.0

        return min(10.0, score)

    async def analyze(self) -> IVMRecommendation:
        """Analyze all tv_ tables and generate recommendations.

        Returns:
            IVMRecommendation with analysis results and setup instructions
        """
        # Check extension availability
        has_extension = await self.check_extension()

        if not has_extension:
            logger.warning("jsonb_ivm extension not available, analysis limited")

        # Discover tv_ tables
        tv_tables = await self.discover_tv_tables()

        if not tv_tables:
            logger.warning("No tv_ tables found")
            return IVMRecommendation(
                total_tv_tables=0,
                incremental_candidates=[],
                full_rebuild_candidates=[],
                estimated_speedup=1.0,
                setup_sql="-- No tv_ tables found",
            )

        # Analyze each table
        candidates: list[IVMCandidate] = []
        for table_name in tv_tables:
            candidate = await self.analyze_table(table_name)
            if candidate:
                candidates.append(candidate)

        # Separate recommendations
        incremental_candidates = [c for c in candidates if c.recommendation == "incremental"]
        full_rebuild_candidates = [c for c in candidates if c.recommendation == "full_rebuild"]

        # Estimate overall speedup
        estimated_speedup = self._estimate_speedup(incremental_candidates)

        # Generate setup SQL (universal sync system)
        setup_sql = self._generate_setup_sql(incremental_candidates)

        # Generate Python sync helpers
        sync_helpers = self._generate_sync_helpers(incremental_candidates)

        # Generate mutation examples
        mutation_examples = self._generate_mutation_examples(incremental_candidates)

        recommendation = IVMRecommendation(
            total_tv_tables=len(tv_tables),
            incremental_candidates=incremental_candidates,
            full_rebuild_candidates=full_rebuild_candidates,
            estimated_speedup=estimated_speedup,
            setup_sql=setup_sql,
            sync_helpers=sync_helpers,
            mutation_examples=mutation_examples,
        )

        logger.info("IVM Analysis complete: %s", recommendation)

        return recommendation

    def _estimate_speedup(self, candidates: list[IVMCandidate]) -> float:
        """Estimate overall speedup from using incremental updates.

        Args:
            candidates: List of tables recommended for incremental updates

        Returns:
            Estimated speedup factor (e.g., 10.0 = 10x faster)
        """
        if not candidates:
            return 1.0

        # Heuristic: Incremental updates typically 10-100x faster
        # Base estimate on complexity scores
        avg_score = sum(c.complexity_score for c in candidates) / len(candidates)

        # Score 5 → 10x, Score 10 → 50x
        estimated_speedup = 10.0 + (avg_score - 5.0) * 8.0

        return max(10.0, min(50.0, estimated_speedup))

    def _generate_setup_sql(self, candidates: list[IVMCandidate]) -> str:
        """Generate SQL for universal sync system (explicit, no triggers).

        Args:
            candidates: List of tables to set up with incremental updates

        Returns:
            SQL script to create universal sync function
        """
        if not candidates:
            return "-- No tables need incremental setup"

        # Generate entity configuration for each table
        entity_configs = []
        for candidate in candidates:
            if not candidate.source_table:
                continue

            # Extract entity name (tv_user → user)
            entity_name = candidate.table_name.replace("tv_", "", 1)

            entity_configs.append(
                f"    ('{entity_name}', '{candidate.table_name}', "
                f"'v_{entity_name}', '{candidate.source_table}'),"
            )

        if not entity_configs:
            return "-- No valid tb_/tv_ pairs found"

        sql_parts = [
            "-- ============================================================================",
            "-- FraiseQL IVM: Universal Sync System (Explicit Control)",
            "-- Generated by FraiseQL IVM Analyzer",
            "-- ============================================================================",
            "-- Pattern: EXPLICIT SYNC (mutation functions call sync, no hidden triggers)",
            "-- Benefits: Full visibility, easy debugging, industrial control",
            "",
            "-- Install jsonb_ivm extension",
            "CREATE EXTENSION IF NOT EXISTS jsonb_ivm;",
            "",
            "-- Create schema for sync infrastructure",
            "CREATE SCHEMA IF NOT EXISTS sync;",
            "",
            "-- Entity configuration table",
            "CREATE TABLE IF NOT EXISTS sync.entity_config (",
            "    entity_type TEXT PRIMARY KEY,",
            "    tv_table TEXT NOT NULL,",
            "    v_view TEXT NOT NULL,",
            "    tb_table TEXT NOT NULL",
            ");",
            "",
            "-- Insert entity configurations",
            "INSERT INTO sync.entity_config (entity_type, tv_table, v_view, tb_table)",
            "VALUES",
        ]

        # Add configurations (remove trailing comma from last one)
        entity_configs[-1] = entity_configs[-1].rstrip(",") + ";"

        sql_parts.extend(entity_configs)

        sql_parts.extend(
            [
                "",
                "-- Sync metrics table",
                "CREATE TABLE IF NOT EXISTS sync.metrics (",
                "    id SERIAL PRIMARY KEY,",
                "    entity_type TEXT NOT NULL,",
                "    operation TEXT NOT NULL,",
                "    record_count INT NOT NULL,",
                "    duration_ms INT NOT NULL,",
                "    timestamp TIMESTAMPTZ DEFAULT NOW()",
                ");",
                "",
                "CREATE INDEX IF NOT EXISTS idx_metrics_entity_time ",
                "ON sync.metrics(entity_type, timestamp DESC);",
                "",
                "-- Universal sync function",
                "CREATE OR REPLACE FUNCTION sync.sync_tv_table(",
                "    p_entity_type TEXT,",
                "    p_ids UUID[],",
                "    p_mode TEXT DEFAULT 'incremental'  -- 'incremental' or 'full'",
                ") RETURNS TABLE(synced_count INT, duration_ms INT) AS $$",
                "DECLARE",
                "    v_config RECORD;",
                "    v_start TIMESTAMPTZ;",
                "    v_duration_ms INT;",
                "    v_synced_count INT;",
                "BEGIN",
                "    v_start := clock_timestamp();",
                "    ",
                "    -- Get entity configuration",
                "    SELECT * INTO v_config",
                "    FROM sync.entity_config",
                "    WHERE entity_type = p_entity_type;",
                "    ",
                "    IF NOT FOUND THEN",
                "        RAISE EXCEPTION 'Unknown entity type: %', p_entity_type;",
                "    END IF;",
                "    ",
                "    IF p_mode = 'incremental' THEN",
                "        -- Incremental update using jsonb_merge_shallow",
                "        EXECUTE format(",
                "            'UPDATE %I SET data = jsonb_merge_shallow(',",
                "            '    data,',",
                "            '    (SELECT data FROM %I WHERE id = %I.id)',",
                "            ')',",
                "            'WHERE id = ANY($1)',",
                "            v_config.tv_table, v_config.v_view, v_config.v_view",
                "        ) USING p_ids;",
                "    ELSE",
                "        -- Full rebuild",
                "        EXECUTE format(",
                "            'UPDATE %I SET data = (SELECT data FROM %I WHERE id = %I.id)',",
                "            'WHERE id = ANY($1)',",
                "            v_config.tv_table, v_config.v_view, v_config.v_view",
                "        ) USING p_ids;",
                "    END IF;",
                "    ",
                "    GET DIAGNOSTICS v_synced_count = ROW_COUNT;",
                "    v_duration_ms := EXTRACT(MILLISECONDS FROM clock_timestamp() - v_start)::INT;",
                "    ",
                "    -- Record metrics",
                "    INSERT INTO sync.metrics (entity_type, operation, record_count, duration_ms)",
                "    VALUES (p_entity_type, p_mode, v_synced_count, v_duration_ms);",
                "    ",
                "    RETURN QUERY SELECT v_synced_count, v_duration_ms;",
                "END;",
                "$$ LANGUAGE plpgsql;",
                "",
                "-- Monitoring view",
                "CREATE OR REPLACE VIEW sync.v_metrics_summary AS",
                "SELECT",
                "    entity_type,",
                "    operation,",
                "    COUNT(*) as total_syncs,",
                "    AVG(duration_ms)::INT as avg_ms,",
                "    MIN(duration_ms) as min_ms,",
                "    MAX(duration_ms) as max_ms,",
                "    SUM(record_count) as total_records",
                "FROM sync.metrics",
                "WHERE timestamp > NOW() - INTERVAL '24 hours'",
                "GROUP BY entity_type, operation",
                "ORDER BY total_syncs DESC;",
                "",
                "-- ============================================================================",
                "-- Setup complete!",
                "-- ============================================================================",
                "-- Usage in mutation functions:",
                "-- ",
                "-- CREATE OR REPLACE FUNCTION app.create_user(...) RETURNS UUID AS $$",
                "-- DECLARE",
                "--     v_user_id UUID;",
                "-- BEGIN",
                "--     -- 1. Insert into tb_user",
                "--     INSERT INTO tb_user (...) VALUES (...) RETURNING id INTO v_user_id;",
                "--     ",
                "--     -- 2. Explicitly sync to tv_user",
                "--     PERFORM sync.sync_tv_table('user', ARRAY[v_user_id], 'incremental');",
                "--     ",
                "--     RETURN v_user_id;",
                "-- END;",
                "-- $$ LANGUAGE plpgsql;",
                "",
            ]
        )

        return "\n".join(sql_parts)

    def _generate_sync_helpers(self, candidates: list[IVMCandidate]) -> str:
        """Generate Python helper functions for explicit sync.

        Args:
            candidates: List of tables needing sync helpers

        Returns:
            Python code for sync helper module
        """
        if not candidates:
            return "# No sync helpers needed"

        helpers = [
            '"""FraiseQL IVM Sync Helpers (Generated).',
            "",
            "These helpers provide explicit sync functions for tv_ table updates.",
            "Use these in your mutation functions for full visibility and control.",
            '"""',
            "",
            "from typing import Any",
            "import logging",
            "",
            "logger = logging.getLogger(__name__)",
            "",
            "",
            "class SyncHelper:",
            '    """Universal sync helper for tv_ table updates."""',
            "",
            "    def __init__(self, connection_pool):",
            "        self.pool = connection_pool",
            "",
            "    async def sync_tv_table(",
            "        self,",
            "        entity_type: str,",
            "        ids: list[Any],",
            "        mode: str = 'incremental'",
            "    ) -> tuple[int, int]:",
            '        """Sync tb_ changes to tv_ table.',
            "",
            "        Args:",
            "            entity_type: Entity name (e.g., 'user', 'post')",
            "            ids: List of IDs to sync",
            "            mode: 'incremental' (fast) or 'full' (rebuild)",
            "",
            "        Returns:",
            "            Tuple of (synced_count, duration_ms)",
            '        """',
            "        async with self.pool.connection() as conn, conn.cursor() as cur:",
            "            await cur.execute(",
            '                "SELECT * FROM sync.sync_tv_table(%s, %s, %s)",',
            "                (entity_type, ids, mode)",
            "            )",
            "            result = await cur.fetchone()",
            "            await conn.commit()",
            "",
            "            synced_count, duration_ms = result",
            "            logger.debug(",
            '                "Synced %d %s records in %dms (mode: %s)",',
            "                synced_count, entity_type, duration_ms, mode",
            "            )",
            "",
            "            return synced_count, duration_ms",
            "",
        ]

        # Generate entity-specific helpers
        for candidate in candidates:
            if not candidate.source_table:
                continue

            entity_name = candidate.table_name.replace("tv_", "", 1)

            helpers.extend(
                [
                    f"    async def sync_{entity_name}(",
                    "        self,",
                    "        ids: list[Any],",
                    "        mode: str = 'incremental'",
                    "    ) -> tuple[int, int]:",
                    f'        """Sync {entity_name} records from {candidate.source_table} '
                    f'to {candidate.table_name}."""',
                    f"        return await self.sync_tv_table('{entity_name}', ids, mode)",
                    "",
                ]
            )

        helpers.extend(
            [
                "    async def get_sync_stats(self, entity_type: str | None = None):",
                '        """Get sync performance statistics."""',
                "        async with self.pool.connection() as conn, conn.cursor() as cur:",
                "            if entity_type:",
                "                await cur.execute(",
                '                    "SELECT * FROM sync.v_metrics_summary '
                'WHERE entity_type = %s",',
                "                    (entity_type,)",
                "                )",
                "            else:",
                '                await cur.execute("SELECT * FROM sync.v_metrics_summary")',
                "",
                "            return await cur.fetchall()",
            ]
        )

        return "\n".join(helpers)

    def _generate_mutation_examples(self, candidates: list[IVMCandidate]) -> str:
        """Generate example mutation functions with explicit sync.

        Args:
            candidates: List of tables needing mutation examples

        Returns:
            Example mutation function code
        """
        if not candidates or not candidates[0].source_table:
            return "# No examples available"

        # Use first candidate for example
        candidate = candidates[0]
        entity_name = candidate.table_name.replace("tv_", "", 1)

        examples = [
            "# ============================================================================",
            "# Example Mutation Functions with Explicit Sync",
            "# ============================================================================",
            "# Pattern: Command (tb_) → Sync → Query (tv_)",
            "# Benefits: Full visibility, easy testing, industrial control",
            "",
            "from fraiseql.ivm.sync_helper import SyncHelper",
            "from uuid import uuid4",
            "",
            "sync = SyncHelper(app.db_pool)",
            "",
            "",
            f"# Example 1: Create {entity_name}",
            f"async def create_{entity_name}(name: str, email: str) -> str:",
            f'    """Create a new {entity_name} with explicit sync.',
            "",
            "    Steps:",
            f"    1. Insert into {candidate.source_table} (command side)",
            f"    2. Explicitly sync to {candidate.table_name} (query side)",
            "    3. Return ID",
            '    """',
            f"    # Step 1: Insert into {candidate.source_table}",
            "    async with app.db_pool.connection() as conn:",
            f"        {entity_name}_id = str(uuid4())",
            "        await conn.execute(",
            f'            "INSERT INTO {candidate.source_table} (id, name, email) '
            'VALUES ($1, $2, $3)",',
            f"            ({entity_name}_id, name, email)",
            "        )",
            "        await conn.commit()",
            "",
            f"    # Step 2: Sync to {candidate.table_name}",
            f"    synced, duration = await sync.sync_{entity_name}([{entity_name}_id])",
            f'    logger.info("Created {entity_name} %s and synced in %dms", '
            f"{entity_name}_id, duration)",
            "",
            f"    return {entity_name}_id",
            "",
            "",
            f"# Example 2: Update {entity_name}",
            f"async def update_{entity_name}({entity_name}_id: str, **updates) -> bool:",
            f'    """Update {entity_name} with explicit incremental sync."""',
            f"    # Step 1: Update {candidate.source_table}",
            "    async with app.db_pool.connection() as conn:",
            "        # Build UPDATE query from updates dict",
            "        set_clause = ', '.join(f'{k} = ${i+2}' for i, k in enumerate(updates.keys()))",
            "        query = f'UPDATE {candidate.source_table} SET {set_clause} WHERE id = $1'",
            "",
            f"        await conn.execute(query, ({entity_name}_id, *updates.values()))",
            "        await conn.commit()",
            "",
            f"    # Step 2: Incremental sync to {candidate.table_name}",
            "    # Only updated fields are merged (fast!)",
            f"    synced, duration = await sync.sync_{entity_name}(",
            f"        [{entity_name}_id],",
            "        mode='incremental'  # 10-100x faster than full rebuild",
            "    )",
            "",
            "    return synced > 0",
            "",
            "",
            f"# Example 3: Delete {entity_name}",
            f"async def delete_{entity_name}({entity_name}_id: str) -> bool:",
            f'    """Delete {entity_name} from both tb_ and tv_ tables."""',
            "    async with app.db_pool.connection() as conn:",
            "        # Delete from both tables",
            f"        await conn.execute('DELETE FROM {candidate.table_name} WHERE id = $1', "
            f"({entity_name}_id,))",
            f"        await conn.execute('DELETE FROM {candidate.source_table} WHERE id = $1', "
            f"({entity_name}_id,))",
            "        await conn.commit()",
            "",
            "    return True",
            "",
            "",
            "# ============================================================================",
            "# Testing Examples",
            "# ============================================================================",
            "",
            "# Test 1: Verify incremental sync works",
            "async def test_incremental_sync():",
            f"    # Create {entity_name}",
            f"    {entity_name}_id = await create_{entity_name}('Alice', 'alice@example.com')",
            "",
            "    # Update only one field",
            f"    await update_{entity_name}({entity_name}_id, name='Alice Smith')",
            "",
            "    # Verify tv_ table has updated data",
            "    async with app.db_pool.connection() as conn:",
            "        result = await conn.execute(",
            f"            'SELECT data FROM {candidate.table_name} WHERE id = $1',",
            f"            ({entity_name}_id,)",
            "        )",
            "        data = await result.fetchone()",
            "",
            "        assert data[0]['name'] == 'Alice Smith'",
            "        assert data[0]['email'] == 'alice@example.com'",
            "",
            "",
            "# Test 2: Performance comparison",
            "async def test_sync_performance():",
            "    import time",
            "",
            f"    # Create test {entity_name}",
            f"    {entity_name}_id = await create_{entity_name}('Bob', 'bob@example.com')",
            "",
            "    # Test incremental (should be fast)",
            "    start = time.time()",
            f"    await sync.sync_{entity_name}([{entity_name}_id], mode='incremental')",
            "    incremental_time = (time.time() - start) * 1000",
            "",
            "    # Test full rebuild (slower)",
            "    start = time.time()",
            f"    await sync.sync_{entity_name}([{entity_name}_id], mode='full')",
            "    full_time = (time.time() - start) * 1000",
            "",
            "    speedup = full_time / incremental_time",
            "    print(f'Incremental: {incremental_time:.2f}ms')",
            "    print(f'Full rebuild: {full_time:.2f}ms')",
            "    print(f'Speedup: {speedup:.1f}x')",
            "",
        ]

        return "\n".join(examples)

    def print_analysis_report(self, recommendation: IVMRecommendation) -> None:
        """Print detailed analysis report.

        Args:
            recommendation: IVM recommendation to report on
        """
        report_lines = [
            "",
            "=" * 80,
            "FraiseQL IVM Analysis Report",
            "=" * 80,
            "",
            f"Total tv_ tables: {recommendation.total_tv_tables}",
            f"Incremental candidates: {len(recommendation.incremental_candidates)}",
            f"Full rebuild: {len(recommendation.full_rebuild_candidates)}",
            f"Estimated speedup: {recommendation.estimated_speedup:.1f}x",
            "",
        ]

        if recommendation.incremental_candidates:
            report_lines.extend(
                [
                    "-" * 80,
                    "Recommended for Incremental Updates (jsonb_merge_shallow)",
                    "-" * 80,
                ]
            )

            for candidate in sorted(
                recommendation.incremental_candidates,
                key=lambda c: c.complexity_score,
                reverse=True,
            ):
                report_lines.append(
                    f"  ✓ {candidate.table_name:30} "
                    f"(rows: {candidate.row_count:>8,}, "
                    f"fields: {candidate.jsonb_field_count:>2}, "
                    f"score: {candidate.complexity_score:.1f})"
                )

        if recommendation.full_rebuild_candidates:
            report_lines.extend(["", "-" * 80, "Keep Full Rebuild", "-" * 80])

            for candidate in recommendation.full_rebuild_candidates:
                report_lines.append(
                    f"  • {candidate.table_name:30} "
                    f"(rows: {candidate.row_count:>8,}, "
                    f"score: {candidate.complexity_score:.1f})"
                )

        report_lines.extend(["", "=" * 80, ""])

        report = "\n".join(report_lines)
        logger.info(report)


async def setup_auto_ivm(
    connection_pool: Any, *, verbose: bool = False, dry_run: bool = False
) -> IVMRecommendation:
    """Analyze tv_ tables and optionally set up incremental maintenance.

    This is the main entry point for auto-IVM setup. Call this during
    application startup to analyze your tv_ tables and get recommendations.

    Args:
        connection_pool: psycopg connection pool
        verbose: If True, print detailed analysis report
        dry_run: If True, only analyze without creating triggers

    Returns:
        IVMRecommendation with analysis and setup status

    Example:
        ```python
        from fraiseql.ivm import setup_auto_ivm

        @app.on_event("startup")
        async def setup_ivm():
            recommendation = await setup_auto_ivm(
                connection_pool=app.db_pool,
                verbose=True,
                dry_run=False  # Set to True to see recommendations only
            )
            print(recommendation)
        ```
    """
    analyzer = IVMAnalyzer(connection_pool)

    # Analyze all tv_ tables
    recommendation = await analyzer.analyze()

    # Print report if verbose
    if verbose:
        analyzer.print_analysis_report(recommendation)

    # Set up triggers if not dry run
    if not dry_run and recommendation.incremental_candidates:
        logger.info(
            "Setting up incremental triggers for %d tables",
            len(recommendation.incremental_candidates),
        )

        try:
            async with connection_pool.connection() as conn:
                await conn.execute(recommendation.setup_sql)
                await conn.commit()

                logger.info(
                    "✓ Successfully set up incremental triggers for %d tv_ tables",
                    len(recommendation.incremental_candidates),
                )
        except Exception as e:
            logger.error("Failed to set up incremental triggers: %s", e)
            logger.info("You can apply the SQL manually:")
            logger.info(recommendation.setup_sql)

    elif dry_run:
        logger.info("Dry run mode: no triggers created")
        logger.info("To apply recommendations, run with dry_run=False")

    return recommendation
