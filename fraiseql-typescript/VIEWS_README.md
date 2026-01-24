# @fraiseql/views - DDL Generation for Table-Backed Views

Production-ready TypeScript library for generating SQL DDL for table-backed views (tv_* for JSON planes, ta_* for Arrow planes).

## Overview

This module provides helper functions to generate DDL for table-backed views, enabling developers to explicitly create materialized views for performance optimization. Following FraiseQL's philosophy of **explicit over implicit**, DDL generation is a **tool developers call**, not an automatic optimization the compiler performs.

**Key Philosophy**: After deciding to use table-backed views (via Phase 9.4 guides), use these tools to generate production-ready DDL.

## Installation

```bash
# Already included in fraiseql package
import { generateTvDdl, loadSchema } from "fraiseql";
```

## Quick Start

### Generate DDL for JSON View (tv_*)

```typescript
import { loadSchema, generateTvDdl } from "fraiseql";
import * as fs from "fs";

// Load your schema
const schema = loadSchema("schema.json");

// Generate DDL for table-backed JSON view
const ddl = generateTvDdl({
  schema,
  entity: "User",
  view: "user_profile",
  refreshStrategy: "trigger-based", // or "scheduled"
});

// Save to file
fs.writeFileSync("tv_user_profile.sql", ddl);

// Deploy
// $ psql < tv_user_profile.sql
```

### Generate DDL for Arrow View (ta_*)

```typescript
const ddl = generateTaDdl({
  schema,
  entity: "User",
  view: "user_stats",
  refreshStrategy: "scheduled",
});

fs.writeFileSync("ta_user_stats.sql", ddl);
```

### Get Refresh Strategy Recommendation

```typescript
const strategy = suggestRefreshStrategy({
  writeVolumePerMinute: 1000,
  latencyRequirementMs: 500,
  readVolumePerSecond: 50,
});
// Returns "trigger-based" for high write volume with strict latency
```

## API Reference

### Functions

#### `loadSchema(filePath: string): SchemaObject`

Load a schema.json file from disk.

**Parameters:**
- `filePath`: Path to schema.json

**Returns:** Parsed SchemaObject

**Throws:** Error if file not found or invalid JSON

```typescript
const schema = loadSchema("schema.json");
```

---

#### `generateTvDdl(options: GenerateTvOptions): string`

Generate DDL for a table-backed JSON view (tv_*).

**Parameters:**
```typescript
interface GenerateTvOptions {
  schema: SchemaObject;              // Loaded schema
  entity: string;                    // Entity name (e.g., "User")
  view: string;                      // View name (e.g., "user_profile")
  refreshStrategy?: "trigger-based" | "scheduled"; // Default: "trigger-based"
  includeCompositionViews?: boolean;  // Default: true
  includeMonitoringFunctions?: boolean; // Default: true
}
```

**Returns:** Complete SQL DDL string containing:
- Table definition with JSONB storage
- Indexes (entity_id, updated_at, is_stale, GIN for JSON)
- Optional composition views for nested relationships
- Optional refresh function (trigger or scheduled)
- Optional monitoring/staleness detection functions

**Example:**
```typescript
const ddl = generateTvDdl({
  schema,
  entity: "User",
  view: "user_profile",
  refreshStrategy: "trigger-based",
  includeCompositionViews: true,
  includeMonitoringFunctions: true,
});
```

---

#### `generateTaDdl(options: GenerateTaOptions): string`

Generate DDL for a table-backed Arrow view (ta_*).

**Parameters:**
```typescript
interface GenerateTaOptions {
  schema: SchemaObject;              // Loaded schema
  entity: string;                    // Entity name
  view: string;                      // View name
  refreshStrategy?: "scheduled" | "trigger-based"; // Default: "scheduled"
  includeMonitoringFunctions?: boolean; // Default: true
}
```

**Returns:** Complete SQL DDL with:
- Table definition with Arrow IPC-encoded columnar storage
- Columns for each entity field storing Arrow RecordBatches
- Batch metadata (row count, size, compression)
- Flight metadata (dictionary encoding, compression codecs)
- Indexes and monitoring functions

**Example:**
```typescript
const ddl = generateTaDdl({
  schema,
  entity: "Order",
  view: "order_stats",
  refreshStrategy: "scheduled",
});
```

---

#### `generateCompositionViews(options: CompositionOptions): string`

Generate SQL for composition helper views.

**Parameters:**
```typescript
interface CompositionOptions {
  schema: SchemaObject;
  entity: string;              // Parent entity (e.g., "User")
  relationships: string[];     // Relationship names to compose
}
```

**Returns:** SQL for:
- Composition views (cv_*) for each relationship
- Batch composition helper function

**Example:**
```typescript
const sql = generateCompositionViews({
  schema,
  entity: "User",
  relationships: ["posts", "comments", "followers"],
});
```

---

#### `suggestRefreshStrategy(options: StrategyOptions): string`

Suggest refresh strategy based on workload characteristics.

**Parameters:**
```typescript
interface StrategyOptions {
  writeVolumePerMinute: number;   // Writes per minute
  latencyRequirementMs: number;   // Required latency in ms
  readVolumePerSecond: number;    // Reads per second
}
```

**Returns:** `"trigger-based"` or `"scheduled"`

**Logic:**
- Returns `"trigger-based"` if:
  - Write volume > 100 writes/min, OR
  - Latency requirement < 500ms, OR
  - High read volume (>10 reads/sec) with strict latency (<1000ms)
- Returns `"scheduled"` otherwise

**Example:**
```typescript
const strategy = suggestRefreshStrategy({
  writeVolumePerMinute: 500,
  latencyRequirementMs: 1000,
  readVolumePerSecond: 50,
}); // "trigger-based"
```

---

#### `validateGeneratedDdl(sql: string): string[]`

Validate generated DDL for syntax errors and common issues.

**Parameters:**
- `sql`: Generated DDL string

**Returns:** Array of validation errors (empty if valid)

**Validates:**
- Balanced parentheses and quotes
- Presence of CREATE statement
- Documentation completeness (warnings)

**Note:** This is basic validation. Execute against a test database for comprehensive validation.

**Example:**
```typescript
const errors = validateGeneratedDdl(ddl);
if (errors.length > 0) {
  console.error("Validation errors:", errors);
} else {
  console.log("✅ DDL is valid");
}
```

---

### Type Definitions

#### `SchemaObject`
Complete schema structure from schema.json:
```typescript
interface SchemaObject {
  types: SchemaType[];
  queries?: Record<string, unknown>;
  mutations?: Record<string, unknown>;
  observers?: Record<string, unknown>;
  [key: string]: unknown;
}
```

#### `SchemaType`
Entity definition:
```typescript
interface SchemaType {
  name: string;
  fields: SchemaField[];
  relationships?: SchemaRelationship[];
}
```

#### `SchemaField`
Entity field:
```typescript
interface SchemaField {
  name: string;
  type: string;
  nullable?: boolean;
}
```

#### `SchemaRelationship`
Entity relationship:
```typescript
interface SchemaRelationship {
  name: string;
  target_entity: string;
  cardinality?: "one" | "many";
}
```

## Generated DDL Structure

### For tv_* (Table-backed JSON Views)

```sql
-- Table definition
CREATE TABLE tv_user_profile (
    view_id BIGSERIAL PRIMARY KEY,
    entity_id INTEGER NOT NULL UNIQUE,
    entity_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    composition_ids TEXT[],
    created_at TIMESTAMP WITH TIME ZONE,
    updated_at TIMESTAMP WITH TIME ZONE,
    view_generated_at TIMESTAMP WITH TIME ZONE,
    is_stale BOOLEAN DEFAULT false,
    staleness_detected_at TIMESTAMP WITH TIME ZONE,
    check_interval INTERVAL DEFAULT '1 hour'
);

-- Indexes
CREATE INDEX idx_tv_user_profile_entity_id ON tv_user_profile(entity_id);
CREATE INDEX idx_tv_user_profile_updated_at ON tv_user_profile(updated_at DESC);
CREATE INDEX idx_tv_user_profile_is_stale ON tv_user_profile(is_stale) WHERE is_stale = true;
CREATE INDEX idx_tv_user_profile_entity_json_gin ON tv_user_profile USING GIN(entity_json);

-- Composition views (optional)
CREATE VIEW cv_User_posts AS ...
CREATE FUNCTION batch_compose_User() ...

-- Refresh logic
CREATE TRIGGER trg_refresh_tv_user_profile ...
-- OR
CREATE FUNCTION refresh_tv_user_profile_batch() ...

-- Monitoring (optional)
CREATE FUNCTION check_staleness_user_profile() ...
CREATE VIEW v_staleness_user_profile ...
```

### For ta_* (Table-backed Arrow Views)

```sql
-- Table definition with Arrow columns
CREATE TABLE ta_user_stats (
    batch_id BIGSERIAL PRIMARY KEY,
    batch_number INTEGER NOT NULL,
    col_id BYTEA NOT NULL,        -- Arrow IPC-encoded
    col_name BYTEA NOT NULL,
    col_email BYTEA NOT NULL,
    -- ... more columns
    row_count INTEGER,
    batch_size_bytes BIGINT,
    compression CHAR(10),
    dictionary_encoded_fields TEXT[],
    field_compression_codecs TEXT[],
    created_at TIMESTAMP WITH TIME ZONE,
    updated_at TIMESTAMP WITH TIME ZONE,
    is_stale BOOLEAN DEFAULT false
);

-- Indexes
CREATE INDEX idx_ta_user_stats_batch_number ON ta_user_stats(batch_number DESC);
CREATE INDEX idx_ta_user_stats_updated_at ON ta_user_stats(updated_at DESC);

-- Refresh logic
CREATE FUNCTION refresh_ta_user_stats_batch() ...

-- Monitoring
CREATE FUNCTION check_staleness_user_stats() ...
```

## Refresh Strategies

### Trigger-Based (Real-Time)

- **When to use**: High write volume, strict latency requirements, need real-time freshness
- **How it works**: Database trigger marks view as stale on every source table change
- **Pros**: Immediate detection of staleness
- **Cons**: Trigger overhead on every write

```sql
CREATE TRIGGER trg_refresh_tv_user_profile
AFTER INSERT OR UPDATE OR DELETE ON User
FOR EACH ROW
EXECUTE FUNCTION refresh_tv_user_profile();
```

### Scheduled (Batch)

- **When to use**: Low write volume, relaxed latency, prefer batch efficiency
- **How it works**: Scheduled job (pg_cron) periodically marks stale entries
- **Pros**: Minimal overhead, batch efficiency
- **Cons**: Delayed staleness detection

```sql
-- Run every 15 minutes
SELECT cron.schedule('refresh-tv-user-profile', '*/15 * * * *',
  'SELECT refresh_tv_user_profile_batch()');
```

## Monitoring Table-Backed Views

### Check Staleness

```sql
-- Get staleness metrics
SELECT * FROM check_staleness_user_profile();
-- Returns: (stale_count, oldest_stale, total_count)

-- View staleness dashboard
SELECT * FROM v_staleness_user_profile;
-- Returns staleness_percent, max_staleness_duration, etc.
```

### Manual Refresh

```sql
-- Trigger refresh of stale entries
SELECT refresh_tv_user_profile_batch();

-- Or for Arrow views
SELECT refresh_ta_user_stats_batch();
```

## Complete Workflow Example

```typescript
import { generateTvDdl, loadSchema, validateGeneratedDdl, suggestRefreshStrategy } from "fraiseql";
import * as fs from "fs";

async function deployTableBackedView() {
  // 1. Load schema
  const schema = loadSchema("schema.json");

  // 2. Get refresh strategy recommendation
  const strategy = suggestRefreshStrategy({
    writeVolumePerMinute: 500,
    latencyRequirementMs: 2000,
    readVolumePerSecond: 100,
  });

  // 3. Generate DDL
  const ddl = generateTvDdl({
    schema,
    entity: "Order",
    view: "order_summary",
    refreshStrategy: strategy,
    includeCompositionViews: true,
    includeMonitoringFunctions: true,
  });

  // 4. Validate
  const errors = validateGeneratedDdl(ddl);
  if (errors.length > 0) {
    console.error("Validation failed:", errors);
    return;
  }

  // 5. Save to file
  fs.writeFileSync("tv_order_summary.sql", ddl);
  console.log("✅ DDL ready for deployment");

  // 6. Deploy (manual or automated)
  // $ psql < tv_order_summary.sql
}

deployTableBackedView();
```

## Error Handling

All functions throw descriptive errors:

```typescript
try {
  const ddl = generateTvDdl({
    schema,
    entity: "NonExistent",
    view: "test",
  });
} catch (error) {
  console.error(error.message);
  // "Entity 'NonExistent' not found in schema"
}
```

## TypeScript Strict Mode

All code is fully typed and passes TypeScript strict mode:
- No `any` types
- No implicit `unknown`
- Full null safety
- Comprehensive JSDoc comments

## Performance

- **Schema loading**: ~1ms for typical schemas
- **DDL generation**: ~2-5ms per view
- **Validation**: ~1ms per view

Suitable for:
- Build-time generation
- CI/CD pipelines
- Interactive development tools
- Web UI generation

## Testing

Comprehensive test suite with 31 tests:

```bash
npm test -- tests/views.test.ts
```

Covers:
- All public functions
- Error handling and edge cases
- Type safety
- End-to-end workflows

## Design Principles

1. **Explicit over Implicit**: Developers decide when to use table-backed views
2. **Production-Ready**: Generated DDL is immediately deployable
3. **No Runtime Dependencies**: Only Node.js built-in modules (fs)
4. **Type Safe**: Full TypeScript strict mode compliance
5. **Well Documented**: JSDoc for all public APIs

## Related Documentation

- **Phase 9.4**: View selection guide (when to use tv_* vs ta_*)
- **Phase 9.5**: DDL generation helper specification
- **View Migration Guide**: Step-by-step deployment checklist
- **Performance Testing Guide**: Measuring view effectiveness

## Contributing

This library is part of the FraiseQL project. Contributions should:
- Maintain TypeScript strict mode compliance
- Include comprehensive JSDoc comments
- Add tests for new functionality
- Follow existing code style

## License

Part of FraiseQL v2 project.
