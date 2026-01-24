/**
 * DDL generation helpers for table-backed views (tv_* and ta_*).
 *
 * This module provides functions to generate SQL DDL for table-backed views,
 * enabling developers to explicitly create materialized views for performance optimization.
 *
 * Following FraiseQL's philosophy of explicit over implicit, these functions are
 * **tools developers call**, not automatic optimizations the compiler performs.
 *
 * @example
 * ```ts
 * import { generateTvDdl, loadSchema } from "@fraiseql/tools/views";
 *
 * const schema = loadSchema("schema.json");
 * const ddl = generateTvDdl({
 *   schema,
 *   entity: "User",
 *   view: "tv_user_profile",
 *   refreshStrategy: "trigger-based"
 * });
 *
 * console.log(ddl);
 * // Output: Ready-to-run SQL with table definition, indexes, and refresh logic
 * ```
 */

import * as fs from "fs";

/**
 * Represents a field in a schema entity.
 */
export interface SchemaField {
  name: string;
  type: string;
  nullable?: boolean;
}

/**
 * Represents a relationship between entities.
 */
export interface SchemaRelationship {
  name: string;
  target_entity: string;
  cardinality?: "one" | "many";
}

/**
 * Represents a top-level type/entity in the schema.
 */
export interface SchemaType {
  name: string;
  fields: SchemaField[];
  relationships?: SchemaRelationship[];
}

/**
 * The complete schema structure (from schema.json).
 */
export interface SchemaObject {
  types: SchemaType[];
  queries?: Record<string, unknown>;
  mutations?: Record<string, unknown>;
  observers?: Record<string, unknown>;
  [key: string]: unknown;
}

/**
 * Options for generating tv_* (Table-backed JSON View) DDL.
 */
export interface GenerateTvOptions {
  /** The loaded schema object */
  schema: SchemaObject;

  /** Entity name (e.g., "User", "Order") */
  entity: string;

  /** View name (e.g., "tv_user_profile", "tv_order_summary") */
  view: string;

  /** Refresh strategy: "trigger-based" for real-time, "scheduled" for batch */
  refreshStrategy?: "trigger-based" | "scheduled";

  /** Include helper composition views for nested relationships (default: true) */
  includeCompositionViews?: boolean;

  /** Include monitoring and staleness-detection functions (default: true) */
  includeMonitoringFunctions?: boolean;
}

/**
 * Options for generating ta_* (Table-backed Arrow View) DDL.
 */
export interface GenerateTaOptions {
  /** The loaded schema object */
  schema: SchemaObject;

  /** Entity name (e.g., "User", "Order") */
  entity: string;

  /** View name (e.g., "ta_user_stats", "ta_order_metrics") */
  view: string;

  /** Refresh strategy: "scheduled" for batch, "trigger-based" for real-time */
  refreshStrategy?: "scheduled" | "trigger-based";

  /** Include monitoring and staleness-detection functions (default: true) */
  includeMonitoringFunctions?: boolean;
}

/**
 * Options for generating composition helper views.
 */
export interface CompositionOptions {
  /** The loaded schema object */
  schema: SchemaObject;

  /** Entity name (e.g., "User", "Order") */
  entity: string;

  /** List of relationship names to generate composition views for */
  relationships: string[];
}

/**
 * Options for suggesting a refresh strategy.
 */
export interface StrategyOptions {
  /** Write volume in writes per minute */
  writeVolumePerMinute: number;

  /** Latency requirement in milliseconds */
  latencyRequirementMs: number;

  /** Read volume in requests per second */
  readVolumePerSecond: number;
}

/**
 * Load a schema.json file from disk.
 *
 * @param filePath - Path to the schema.json file
 * @returns Parsed SchemaObject
 *
 * @throws Error if file does not exist or is not valid JSON
 *
 * @example
 * ```ts
 * const schema = loadSchema("./schema.json");
 * console.log(schema.types.length); // number of types
 * ```
 */
export function loadSchema(filePath: string): SchemaObject {
  try {
    const content = fs.readFileSync(filePath, "utf-8");
    const parsed = JSON.parse(content) as SchemaObject;
    return parsed;
  } catch (error) {
    if (error instanceof SyntaxError) {
      throw new Error(`Invalid JSON in schema file ${filePath}: ${error.message}`);
    }
    const errorObj = error as Record<string, unknown>;
    if (errorObj.code === "ENOENT") {
      throw new Error(`Schema file not found: ${filePath}`);
    }
    throw new Error(`Failed to load schema from ${filePath}: ${String(error)}`);
  }
}

/**
 * Generate DDL for a table-backed JSON view (tv_*).
 *
 * Generates a complete SQL file containing:
 * - Table definition with JSONB storage and metadata columns
 * - Indexes for common access patterns (entity_id, updated_at, is_stale)
 * - GIN index for efficient JSONB queries
 * - Optional composition views for nested relationships
 * - Optional refresh function and trigger/scheduler
 * - Optional monitoring functions for staleness detection
 *
 * @param options - Configuration for DDL generation
 * @returns Complete, ready-to-run SQL DDL
 *
 * @throws Error if entity not found in schema
 *
 * @example
 * ```ts
 * const schema = loadSchema("schema.json");
 * const ddl = generateTvDdl({
 *   schema,
 *   entity: "User",
 *   view: "tv_user_profile",
 *   refreshStrategy: "trigger-based",
 *   includeCompositionViews: true,
 *   includeMonitoringFunctions: true
 * });
 *
 * fs.writeFileSync("tv_user_profile.sql", ddl);
 * ```
 */
export function generateTvDdl(options: GenerateTvOptions): string {
  const {
    schema,
    entity,
    view,
    refreshStrategy = "trigger-based",
    includeCompositionViews = true,
    includeMonitoringFunctions = true,
  } = options;

  // Validate inputs
  validateInputs(schema, entity, view);

  const entityDef = findEntity(schema, entity);
  if (!entityDef) {
    throw new Error(`Entity '${entity}' not found in schema`);
  }

  // Start building the DDL
  let ddl = `-- ============================================================================\n`;
  ddl += `-- Table-backed JSON view: tv_${view}\n`;
  ddl += `-- Entity: ${entity}\n`;
  ddl += `-- Refresh Strategy: ${refreshStrategy}\n`;
  ddl += `-- Generated at: ${new Date().toISOString()}\n`;
  ddl += `-- ============================================================================\n\n`;

  // Add table definition
  ddl += generateTvTableDefinition(entity, view);

  // Add composition views if requested
  if (includeCompositionViews && entityDef.relationships && entityDef.relationships.length > 0) {
    ddl += "\n";
    ddl += generateCompositionViews({
      schema,
      entity,
      relationships: entityDef.relationships.map((r) => r.name),
    });
  }

  // Add refresh function based on strategy
  if (refreshStrategy === "trigger-based") {
    ddl += "\n";
    ddl += generateTvRefreshTrigger(entity, view);
  } else {
    ddl += "\n";
    ddl += generateTvRefreshScheduled(entity, view);
  }

  // Add monitoring functions if requested
  if (includeMonitoringFunctions) {
    ddl += "\n";
    ddl += generateMonitoringFunctions(entity, view);
  }

  return ddl;
}

/**
 * Generate DDL for a table-backed Arrow view (ta_*).
 *
 * Generates a complete SQL file containing:
 * - Table definition with Arrow IPC-encoded columnar storage
 * - Columns for each entity field storing Arrow RecordBatches
 * - Batch metadata (row count, size, compression)
 * - Indexes for batch_number and updated_at
 * - Optional refresh function (scheduled or trigger-based)
 * - Optional monitoring functions
 *
 * @param options - Configuration for DDL generation
 * @returns Complete, ready-to-run SQL DDL
 *
 * @throws Error if entity not found in schema
 *
 * @example
 * ```ts
 * const schema = loadSchema("schema.json");
 * const ddl = generateTaDdl({
 *   schema,
 *   entity: "User",
 *   view: "ta_user_stats",
 *   refreshStrategy: "scheduled",
 *   includeMonitoringFunctions: true
 * });
 *
 * fs.writeFileSync("ta_user_stats.sql", ddl);
 * ```
 */
export function generateTaDdl(options: GenerateTaOptions): string {
  const {
    schema,
    entity,
    view,
    refreshStrategy = "scheduled",
    includeMonitoringFunctions = true,
  } = options;

  // Validate inputs
  validateInputs(schema, entity, view);

  const entityDef = findEntity(schema, entity);
  if (!entityDef) {
    throw new Error(`Entity '${entity}' not found in schema`);
  }

  // Start building the DDL
  let ddl = `-- ============================================================================\n`;
  ddl += `-- Table-backed Arrow view: ta_${view}\n`;
  ddl += `-- Entity: ${entity}\n`;
  ddl += `-- Refresh Strategy: ${refreshStrategy}\n`;
  ddl += `-- Generated at: ${new Date().toISOString()}\n`;
  ddl += `-- ============================================================================\n\n`;

  // Add table definition
  ddl += generateTaTableDefinition(entity, view, entityDef.fields);

  // Add refresh function based on strategy
  if (refreshStrategy === "trigger-based") {
    ddl += "\n";
    ddl += generateTaRefreshTrigger(entity, view);
  } else {
    ddl += "\n";
    ddl += generateTaRefreshScheduled(entity, view);
  }

  // Add monitoring functions if requested
  if (includeMonitoringFunctions) {
    ddl += "\n";
    ddl += generateMonitoringFunctions(entity, view);
  }

  return ddl;
}

/**
 * Generate SQL for composition helper views.
 *
 * Creates views (cv_*) that efficiently load related entities for composition
 * into parent views. Also generates a batch composition helper function.
 *
 * @param options - Configuration for composition view generation
 * @returns SQL DDL for composition views and helper functions
 *
 * @example
 * ```ts
 * const sql = generateCompositionViews({
 *   schema,
 *   entity: "User",
 *   relationships: ["posts", "comments"]
 * });
 * ```
 */
export function generateCompositionViews(options: CompositionOptions): string {
  const { entity, relationships } = options;

  if (!relationships || relationships.length === 0) {
    return `-- No composition views generated (no relationships specified)\n`;
  }

  let sql = `-- Composition helper views for ${entity} relationships\n`;
  sql += `-- Purpose: Provide efficient queries for loading nested relationship data\n`;
  sql += `-- These views support loading related entities for composition into parent views\n\n`;

  // Generate views for each relationship
  for (const relationship of relationships) {
    sql += `-- Composition view for relationship: ${relationship}\n`;
    sql += `DROP VIEW IF EXISTS cv_${entity}_${relationship} CASCADE;\n\n`;

    sql += `CREATE VIEW cv_${entity}_${relationship} AS\n`;
    sql += `SELECT\n`;
    sql += `    parent.entity_id AS parent_entity_id,\n`;
    sql += `    related.entity_id AS related_entity_id,\n`;
    sql += `    related.entity_json AS related_json,\n`;
    sql += `    parent.updated_at AS parent_updated_at,\n`;
    sql += `    related.updated_at AS related_updated_at\n`;
    sql += `FROM\n`;
    sql += `    tv_${entity} parent\n`;
    sql += `LEFT JOIN\n`;
    sql += `    tv_${relationship} related\n`;
    sql += `    ON parent.entity_id = related.entity_id\n`;
    sql += `WHERE\n`;
    sql += `    parent.view_generated_at IS NOT NULL\n`;
    sql += `    AND (related.view_generated_at IS NOT NULL OR related.entity_id IS NULL);\n\n`;

    sql += `COMMENT ON VIEW cv_${entity}_${relationship} IS\n`;
    sql += `    'Composition view for loading related entities for ${entity}.${relationship}';\n\n`;
  }

  // Generate batch composition helper function
  sql += `-- Batch composition helper function\n`;
  sql += `-- Efficiently loads related entities for a batch of parent IDs\n`;
  sql += `CREATE OR REPLACE FUNCTION batch_compose_${entity}(\n`;
  sql += `    parent_ids INTEGER[]\n`;
  sql += `)\n`;
  sql += `RETURNS TABLE (\n`;
  sql += `    parent_id INTEGER,\n`;
  sql += `    entity_json JSONB,\n`;
  sql += `    composed_json JSONB\n`;
  sql += `) AS $$\n`;
  sql += `BEGIN\n`;
  sql += `    RETURN QUERY\n`;
  sql += `    SELECT\n`;
  sql += `        p.entity_id,\n`;
  sql += `        p.entity_json,\n`;
  sql += `        jsonb_build_object(\n`;

  const relationshipParts = relationships.map((rel) => {
    return `            '${rel}', COALESCE(\n` +
      `                (\n` +
      `                    SELECT jsonb_agg(r.entity_json)\n` +
      `                    FROM tv_${rel} r\n` +
      `                    WHERE r.entity_id = ANY(\n` +
      `                        SELECT related_entity_id\n` +
      `                        FROM cv_${entity}_${rel}\n` +
      `                        WHERE parent_entity_id = p.entity_id\n` +
      `                    )\n` +
      `                ),\n` +
      `                'null'::jsonb\n` +
      `            )`;
  });

  sql += relationshipParts.join(",\n");
  sql += "\n        ) AS composed\n";
  sql += `    FROM\n`;
  sql += `        tv_${entity} p\n`;
  sql += `    WHERE\n`;
  sql += `        p.entity_id = ANY(parent_ids)\n`;
  sql += `    ORDER BY\n`;
  sql += `        array_position(parent_ids, p.entity_id);\n`;
  sql += `END;\n`;
  sql += `$$ LANGUAGE plpgsql STABLE PARALLEL SAFE;\n\n`;

  sql += `COMMENT ON FUNCTION batch_compose_${entity}(INTEGER[]) IS\n`;
  sql += `    'Batch composition helper for loading related ${entity} entities with all relationships';\n`;

  return sql;
}

/**
 * Suggest a refresh strategy based on workload characteristics.
 *
 * Returns "trigger-based" for high-frequency updates or strict latency requirements,
 * and "scheduled" for lower write volumes or when eventual consistency is acceptable.
 *
 * This is an informational function to help developers choose the right strategy.
 * The actual DDL generation respects the strategy you pass explicitly.
 *
 * @param options - Workload characteristics
 * @returns Suggested strategy: "trigger-based" or "scheduled"
 *
 * @example
 * ```ts
 * const strategy = suggestRefreshStrategy({
 *   writeVolumePerMinute: 1000,
 *   latencyRequirementMs: 100,
 *   readVolumePerSecond: 50
 * });
 * console.log(strategy); // "trigger-based"
 * ```
 */
export function suggestRefreshStrategy(options: StrategyOptions): string {
  const { writeVolumePerMinute, latencyRequirementMs, readVolumePerSecond } = options;

  // Trigger-based if:
  // - High write volume (>100 writes/min)
  // - OR strict latency requirement (<500ms)
  // - OR high read volume with strict latency
  if (
    writeVolumePerMinute > 100 ||
    latencyRequirementMs < 500 ||
    (readVolumePerSecond > 10 && latencyRequirementMs < 1000)
  ) {
    return "trigger-based";
  }

  // Scheduled if:
  // - Low write volume (<100 writes/min)
  // - AND relaxed latency (>500ms)
  return "scheduled";
}

/**
 * Validate generated DDL for syntax errors and common issues.
 *
 * Performs basic validation including:
 * - Checks for matching parentheses and quotes
 * - Validates SQL keywords are present
 * - Warns about potentially incomplete DDL
 *
 * Note: This is not a full SQL parser. For comprehensive validation,
 * execute the DDL against a test database.
 *
 * @param sql - Generated DDL SQL string
 * @returns Array of validation errors (empty if valid)
 *
 * @example
 * ```ts
 * const ddl = generateTvDdl({ schema, entity: "User", view: "tv_user" });
 * const errors = validateGeneratedDdl(ddl);
 * if (errors.length > 0) {
 *   console.error("Validation errors:", errors);
 * }
 * ```
 */
export function validateGeneratedDdl(sql: string): string[] {
  const errors: string[] = [];

  // Check for minimum required content
  if (!sql || sql.trim().length === 0) {
    errors.push("Generated DDL is empty");
    return errors;
  }

  // Check for balanced parentheses
  let parenCount = 0;
  for (const char of sql) {
    if (char === "(") parenCount++;
    if (char === ")") parenCount--;
    if (parenCount < 0) {
      errors.push("Unbalanced parentheses: closing paren without opening");
      break;
    }
  }
  if (parenCount > 0) {
    errors.push(`Unbalanced parentheses: ${parenCount} unclosed opening paren`);
  }

  // Check for balanced quotes
  let singleQuoteCount = 0;
  let doubleQuoteCount = 0;
  for (const char of sql) {
    if (char === "'" && (sql.indexOf(char) === sql.lastIndexOf(char) || !isEscaped(sql, sql.indexOf(char)))) {
      singleQuoteCount++;
    }
    if (char === '"') {
      doubleQuoteCount++;
    }
  }

  if (singleQuoteCount % 2 !== 0) {
    errors.push("Unbalanced single quotes");
  }
  if (doubleQuoteCount % 2 !== 0) {
    errors.push("Unbalanced double quotes");
  }

  // Check for basic SQL structure
  const upperSql = sql.toUpperCase();
  if (!upperSql.includes("CREATE")) {
    errors.push("No CREATE statement found in DDL");
  }

  // Warn about potential issues
  if (!upperSql.includes("COMMENT")) {
    errors.push("Warning: No COMMENT statements found (consider adding documentation)");
  }

  return errors;
}

// ============================================================================
// Private helper functions
// ============================================================================

/**
 * Validate common input parameters.
 */
function validateInputs(schema: SchemaObject, entity: string, view: string): void {
  if (!schema || !schema.types || schema.types.length === 0) {
    throw new Error("Invalid schema: missing types array");
  }

  if (!entity || typeof entity !== "string") {
    throw new Error("Invalid entity: must be a non-empty string");
  }

  if (!view || typeof view !== "string") {
    throw new Error("Invalid view: must be a non-empty string");
  }

  if (!/^[a-z_][a-z0-9_]*$/i.test(entity)) {
    throw new Error(`Invalid entity name '${entity}': must start with letter/underscore, contain only alphanumeric and underscore`);
  }

  if (!/^[a-z_][a-z0-9_]*$/i.test(view)) {
    throw new Error(`Invalid view name '${view}': must start with letter/underscore, contain only alphanumeric and underscore`);
  }
}

/**
 * Find an entity definition by name in the schema.
 */
function findEntity(schema: SchemaObject, entityName: string): SchemaType | undefined {
  return schema.types.find((t) => t.name === entityName);
}

/**
 * Check if a character at index is escaped.
 */
function isEscaped(str: string, index: number): boolean {
  let backslashCount = 0;
  let i = index - 1;
  while (i >= 0 && str[i] === "\\") {
    backslashCount++;
    i--;
  }
  return backslashCount % 2 === 1;
}

/**
 * Generate the table definition for tv_*.
 */
function generateTvTableDefinition(entity: string, view: string): string {
  let sql = `DROP TABLE IF EXISTS tv_${view} CASCADE;\n\n`;

  sql += `CREATE TABLE tv_${view} (\n`;
  sql += `    -- View metadata\n`;
  sql += `    view_id BIGSERIAL PRIMARY KEY,\n`;
  sql += `    entity_id INTEGER NOT NULL UNIQUE,\n\n`;

  sql += `    -- Payload storage\n`;
  sql += `    entity_json JSONB NOT NULL DEFAULT '{}'::jsonb,\n\n`;

  sql += `    -- Composition tracking (for nested relationship views)\n`;
  sql += `    composition_ids TEXT[] DEFAULT ARRAY[]::TEXT[],\n\n`;

  sql += `    -- Materialization metadata\n`;
  sql += `    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,\n`;
  sql += `    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,\n`;
  sql += `    view_generated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,\n\n`;

  sql += `    -- Data quality tracking\n`;
  sql += `    is_stale BOOLEAN DEFAULT false,\n`;
  sql += `    staleness_detected_at TIMESTAMP WITH TIME ZONE,\n\n`;

  sql += `    -- Indexing hints\n`;
  sql += `    check_interval INTERVAL DEFAULT '1 hour'\n`;
  sql += `);\n\n`;

  // Indexes
  sql += `-- Indexes for common access patterns\n`;
  sql += `CREATE INDEX IF NOT EXISTS idx_tv_${view}_entity_id\n`;
  sql += `    ON tv_${view}(entity_id);\n\n`;

  sql += `CREATE INDEX IF NOT EXISTS idx_tv_${view}_updated_at\n`;
  sql += `    ON tv_${view}(updated_at DESC);\n\n`;

  sql += `CREATE INDEX IF NOT EXISTS idx_tv_${view}_is_stale\n`;
  sql += `    ON tv_${view}(is_stale)\n`;
  sql += `    WHERE is_stale = true;\n\n`;

  // JSONB index
  sql += `-- JSONB index for efficient JSON queries\n`;
  sql += `CREATE INDEX IF NOT EXISTS idx_tv_${view}_entity_json_gin\n`;
  sql += `    ON tv_${view} USING GIN(entity_json);\n\n`;

  // Composition index
  sql += `-- Composition tracking index\n`;
  sql += `CREATE INDEX IF NOT EXISTS idx_tv_${view}_composition_ids\n`;
  sql += `    ON tv_${view} USING GIN(composition_ids);\n\n`;

  // Comments
  sql += `-- Comments for documentation\n`;
  sql += `COMMENT ON TABLE tv_${view} IS\n`;
  sql += `    'Table-backed view storing ${entity} entities as JSONB for fast retrieval';\n`;
  sql += `COMMENT ON COLUMN tv_${view}.entity_id IS\n`;
  sql += `    'Reference to the original entity ID in the source table';\n`;
  sql += `COMMENT ON COLUMN tv_${view}.entity_json IS\n`;
  sql += `    'Complete JSON representation of the entity with all scalar and relationship fields';\n`;
  sql += `COMMENT ON COLUMN tv_${view}.is_stale IS\n`;
  sql += `    'Flag indicating if this view entry needs refresh due to source data changes';\n`;
  sql += `COMMENT ON COLUMN tv_${view}.composition_ids IS\n`;
  sql += `    'IDs of composed views that include this entity';\n`;

  return sql;
}

/**
 * Generate the table definition for ta_*.
 */
function generateTaTableDefinition(entity: string, view: string, fields: SchemaField[]): string {
  let sql = `DROP TABLE IF EXISTS ta_${view} CASCADE;\n\n`;

  sql += `CREATE TABLE ta_${view} (\n`;
  sql += `    -- View metadata\n`;
  sql += `    batch_id BIGSERIAL PRIMARY KEY,\n`;
  sql += `    batch_number INTEGER NOT NULL,\n\n`;

  sql += `    -- Arrow columnar storage\n`;
  sql += `    -- Each column stores Arrow IPC-encoded RecordBatch for the field\n`;

  for (const field of fields) {
    sql += `    col_${field.name} BYTEA NOT NULL DEFAULT ''::bytea,\n`;
  }

  sql += `\n    -- Batch metadata\n`;
  sql += `    row_count INTEGER NOT NULL DEFAULT 0,\n`;
  sql += `    batch_size_bytes BIGINT NOT NULL DEFAULT 0,\n`;
  sql += `    compression CHAR(10) DEFAULT 'none',\n\n`;

  sql += `    -- Flight metadata\n`;
  sql += `    dictionary_encoded_fields TEXT[] DEFAULT ARRAY[]::TEXT[],\n`;
  sql += `    field_compression_codecs TEXT[] DEFAULT ARRAY[]::TEXT[],\n\n`;

  sql += `    -- Materialization metadata\n`;
  sql += `    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,\n`;
  sql += `    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,\n`;
  sql += `    view_generated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,\n\n`;

  sql += `    -- Refresh tracking\n`;
  sql += `    is_stale BOOLEAN DEFAULT false,\n`;
  sql += `    staleness_detected_at TIMESTAMP WITH TIME ZONE,\n`;
  sql += `    last_materialized_row_count BIGINT,\n\n`;

  sql += `    -- Performance hints\n`;
  sql += `    estimated_decode_time_ms INTEGER,\n`;
  sql += `    check_interval INTERVAL DEFAULT '30 minutes'\n`;
  sql += `);\n\n`;

  // Indexes
  sql += `-- Indexes for common access patterns\n`;
  sql += `CREATE INDEX IF NOT EXISTS idx_ta_${view}_batch_number\n`;
  sql += `    ON ta_${view}(batch_number DESC);\n\n`;

  sql += `CREATE INDEX IF NOT EXISTS idx_ta_${view}_updated_at\n`;
  sql += `    ON ta_${view}(updated_at DESC);\n\n`;

  sql += `CREATE INDEX IF NOT EXISTS idx_ta_${view}_is_stale\n`;
  sql += `    ON ta_${view}(is_stale)\n`;
  sql += `    WHERE is_stale = true;\n\n`;

  sql += `CREATE INDEX IF NOT EXISTS idx_ta_${view}_row_count\n`;
  sql += `    ON ta_${view}(row_count DESC);\n\n`;

  // Comments
  sql += `-- Comments for documentation\n`;
  sql += `COMMENT ON TABLE ta_${view} IS\n`;
  sql += `    'Table-backed Arrow view storing ${entity} entities as Arrow IPC RecordBatches for efficient columnar streaming';\n`;
  sql += `COMMENT ON COLUMN ta_${view}.batch_id IS\n`;
  sql += `    'Unique identifier for this Arrow batch';\n`;
  sql += `COMMENT ON COLUMN ta_${view}.batch_number IS\n`;
  sql += `    'Sequential batch number for ordering in Flight responses';\n`;
  sql += `COMMENT ON COLUMN ta_${view}.row_count IS\n`;
  sql += `    'Number of rows encoded in this batch';\n`;
  sql += `COMMENT ON COLUMN ta_${view}.batch_size_bytes IS\n`;
  sql += `    'Total size in bytes of Arrow-encoded data across all columns';\n`;
  sql += `COMMENT ON COLUMN ta_${view}.is_stale IS\n`;
  sql += `    'Flag indicating if this view entry needs refresh due to source data changes';\n`;
  sql += `COMMENT ON COLUMN ta_${view}.compression IS\n`;
  sql += `    'Compression codec used for Arrow buffers (none, snappy, lz4, zstd)';\n`;

  return sql;
}

/**
 * Generate refresh trigger for tv_*.
 */
function generateTvRefreshTrigger(entity: string, view: string): string {
  let sql = `-- Refresh trigger for trigger-based updates\n`;
  sql += `-- Called when the source entity changes\n\n`;

  sql += `CREATE OR REPLACE FUNCTION refresh_tv_${view}()\n`;
  sql += `RETURNS TRIGGER AS $$\n`;
  sql += `BEGIN\n`;
  sql += `    UPDATE tv_${view}\n`;
  sql += `    SET\n`;
  sql += `        updated_at = CURRENT_TIMESTAMP,\n`;
  sql += `        is_stale = true,\n`;
  sql += `        staleness_detected_at = CURRENT_TIMESTAMP\n`;
  sql += `    WHERE entity_id = NEW.id;\n\n`;
  sql += `    RETURN NEW;\n`;
  sql += `END;\n`;
  sql += `$$ LANGUAGE plpgsql;\n\n`;

  sql += `COMMENT ON FUNCTION refresh_tv_${view}() IS\n`;
  sql += `    'Trigger function to mark ${entity} entries as stale when source data changes';\n\n`;

  sql += `-- Create trigger on the source entity table\n`;
  sql += `-- Assumes source table is named '${entity}' with 'id' column\n`;
  sql += `DROP TRIGGER IF EXISTS trg_refresh_tv_${view} ON ${entity};\n\n`;

  sql += `CREATE TRIGGER trg_refresh_tv_${view}\n`;
  sql += `AFTER INSERT OR UPDATE OR DELETE ON ${entity}\n`;
  sql += `FOR EACH ROW\n`;
  sql += `EXECUTE FUNCTION refresh_tv_${view}();\n`;

  return sql;
}

/**
 * Generate scheduled refresh for tv_*.
 */
function generateTvRefreshScheduled(entity: string, view: string): string {
  let sql = `-- Batch refresh function for scheduled updates\n`;
  sql += `-- Can be called by pg_cron or external scheduler\n\n`;

  sql += `CREATE OR REPLACE FUNCTION refresh_tv_${view}_batch()\n`;
  sql += `RETURNS TABLE(refreshed_count BIGINT) AS $$\n`;
  sql += `BEGIN\n`;
  sql += `    UPDATE tv_${view}\n`;
  sql += `    SET\n`;
  sql += `        updated_at = CURRENT_TIMESTAMP,\n`;
  sql += `        is_stale = false,\n`;
  sql += `        staleness_detected_at = NULL\n`;
  sql += `    WHERE is_stale = true;\n\n`;
  sql += `    RETURN QUERY SELECT COUNT(*) FROM tv_${view} WHERE updated_at = CURRENT_TIMESTAMP;\n`;
  sql += `END;\n`;
  sql += `$$ LANGUAGE plpgsql VOLATILE;\n\n`;

  sql += `COMMENT ON FUNCTION refresh_tv_${view}_batch() IS\n`;
  sql += `    'Batch refresh function for scheduled updates of ${entity} view';\n\n`;

  sql += `-- Schedule this function with pg_cron (requires pg_cron extension)\n`;
  sql += `-- Example: SELECT cron.schedule('refresh-tv-${view}', '*/15 * * * *', 'SELECT refresh_tv_${view}_batch()');\n`;

  return sql;
}

/**
 * Generate refresh trigger for ta_*.
 */
function generateTaRefreshTrigger(entity: string, view: string): string {
  let sql = `-- Refresh trigger for trigger-based updates\n`;
  sql += `-- Called when the source entity changes\n\n`;

  sql += `CREATE OR REPLACE FUNCTION refresh_ta_${view}()\n`;
  sql += `RETURNS TRIGGER AS $$\n`;
  sql += `BEGIN\n`;
  sql += `    -- Mark Arrow batches as stale when source changes\n`;
  sql += `    UPDATE ta_${view}\n`;
  sql += `    SET\n`;
  sql += `        updated_at = CURRENT_TIMESTAMP,\n`;
  sql += `        is_stale = true,\n`;
  sql += `        staleness_detected_at = CURRENT_TIMESTAMP\n`;
  sql += `    WHERE batch_id IN (\n`;
  sql += `        SELECT DISTINCT batch_id FROM ta_${view}\n`;
  sql += `        WHERE row_count > 0\n`;
  sql += `        LIMIT 100  -- Safety limit to prevent large updates\n`;
  sql += `    );\n\n`;
  sql += `    RETURN NEW;\n`;
  sql += `END;\n`;
  sql += `$$ LANGUAGE plpgsql;\n\n`;

  sql += `COMMENT ON FUNCTION refresh_ta_${view}() IS\n`;
  sql += `    'Trigger function to mark ${entity} Arrow batches as stale when source data changes';\n\n`;

  sql += `-- Create trigger on the source entity table\n`;
  sql += `-- Assumes source table is named '${entity}' with 'id' column\n`;
  sql += `DROP TRIGGER IF EXISTS trg_refresh_ta_${view} ON ${entity};\n\n`;

  sql += `CREATE TRIGGER trg_refresh_ta_${view}\n`;
  sql += `AFTER INSERT OR UPDATE OR DELETE ON ${entity}\n`;
  sql += `FOR EACH ROW\n`;
  sql += `EXECUTE FUNCTION refresh_ta_${view}();\n`;

  return sql;
}

/**
 * Generate scheduled refresh for ta_*.
 */
function generateTaRefreshScheduled(entity: string, view: string): string {
  let sql = `-- Batch refresh function for scheduled updates\n`;
  sql += `-- Can be called by pg_cron or external scheduler\n\n`;

  sql += `CREATE OR REPLACE FUNCTION refresh_ta_${view}_batch()\n`;
  sql += `RETURNS TABLE(refreshed_count BIGINT) AS $$\n`;
  sql += `BEGIN\n`;
  sql += `    UPDATE ta_${view}\n`;
  sql += `    SET\n`;
  sql += `        updated_at = CURRENT_TIMESTAMP,\n`;
  sql += `        is_stale = false,\n`;
  sql += `        staleness_detected_at = NULL\n`;
  sql += `    WHERE is_stale = true;\n\n`;
  sql += `    RETURN QUERY SELECT COUNT(*) FROM ta_${view} WHERE updated_at = CURRENT_TIMESTAMP;\n`;
  sql += `END;\n`;
  sql += `$$ LANGUAGE plpgsql VOLATILE;\n\n`;

  sql += `COMMENT ON FUNCTION refresh_ta_${view}_batch() IS\n`;
  sql += `    'Batch refresh function for scheduled updates of ${entity} Arrow view';\n\n`;

  sql += `-- Schedule this function with pg_cron (requires pg_cron extension)\n`;
  sql += `-- Example: SELECT cron.schedule('refresh-ta-${view}', '*/30 * * * *', 'SELECT refresh_ta_${view}_batch()');\n`;

  return sql;
}

/**
 * Generate monitoring and staleness-detection functions.
 */
function generateMonitoringFunctions(entity: string, view: string): string {
  let sql = `-- Monitoring and staleness-detection functions\n\n`;

  sql += `CREATE OR REPLACE FUNCTION check_staleness_${view}()\n`;
  sql += `RETURNS TABLE(\n`;
  sql += `    stale_count BIGINT,\n`;
  sql += `    oldest_stale TIMESTAMP WITH TIME ZONE,\n`;
  sql += `    total_count BIGINT\n`;
  sql += `) AS $$\n`;
  sql += `BEGIN\n`;
  sql += `    RETURN QUERY\n`;
  sql += `    SELECT\n`;
  sql += `        COUNT(*) FILTER (WHERE is_stale = true),\n`;
  sql += `        MIN(staleness_detected_at) FILTER (WHERE is_stale = true),\n`;
  sql += `        COUNT(*)\n`;
  sql += `    FROM ${view};\n`;
  sql += `END;\n`;
  sql += `$$ LANGUAGE plpgsql STABLE PARALLEL SAFE;\n\n`;

  sql += `COMMENT ON FUNCTION check_staleness_${view}() IS\n`;
  sql += `    'Check staleness metrics for ${entity} view';\n\n`;

  sql += `-- View for easy staleness monitoring\n`;
  sql += `CREATE OR REPLACE VIEW v_staleness_${view} AS\n`;
  sql += `SELECT\n`;
  sql += `    COUNT(*) FILTER (WHERE is_stale = true) as stale_entries,\n`;
  sql += `    COUNT(*) FILTER (WHERE is_stale = false) as fresh_entries,\n`;
  sql += `    COUNT(*) as total_entries,\n`;
  sql += `    ROUND(100.0 * COUNT(*) FILTER (WHERE is_stale = true) / NULLIF(COUNT(*), 0), 2) as staleness_percent,\n`;
  sql += `    MAX(CURRENT_TIMESTAMP - staleness_detected_at) as max_staleness_duration\n`;
  sql += `FROM ${view};\n\n`;

  sql += `COMMENT ON VIEW v_staleness_${view} IS\n`;
  sql += `    'Staleness metrics for ${entity} view';\n`;

  return sql;
}
