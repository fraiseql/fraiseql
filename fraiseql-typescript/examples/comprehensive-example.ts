/**
 * Comprehensive TypeScript Example of FraiseQL DDL Generation
 *
 * This example demonstrates:
 * - Loading schemas and parsing schema structure
 * - Generating both tv_* (JSON views) and ta_* (Arrow columnar views)
 * - Working with related entities and composition views
 * - Smart refresh strategy recommendations
 * - Full validation and error handling
 * - Production-ready deployment patterns
 *
 * Usage:
 *   npx ts-node examples/comprehensive-example.ts
 *   OR
 *   npm run build && node dist/examples/comprehensive-example.js
 */

import * as fs from "fs";
import * as path from "path";
import {
  loadSchema,
  generateTvDdl,
  generateTaDdl,
  generateCompositionViews,
  suggestRefreshStrategy,
  validateGeneratedDdl,
  type SchemaObject,
  type GenerateTvOptions,
  type GenerateTaOptions,
  type SuggestRefreshStrategyOptions,
} from "../src/views";

/**
 * Helper function to print formatted section headers
 */
function printHeader(text: string, char = "="): void {
  console.log(`\n${char.repeat(80)}`);
  console.log(`  ${text}`);
  console.log(`${char.repeat(80)}\n`);
}

/**
 * Helper function to save DDL with header comments
 */
function saveDdlFile(
  ddl: string,
  outputPath: string,
  schemaName: string,
  viewName: string
): void {
  const header = [
    `// FraiseQL DDL Generation Output`,
    `// Schema: ${schemaName}`,
    `// View: ${viewName}`,
    `// Generated: ${new Date().toISOString()}`,
    `// See: https://fraiseql.dev/docs/views`,
    ``,
  ].join("\n");

  const content = `${header}\n${ddl}`;
  fs.writeFileSync(outputPath, content, "utf-8");
  console.log(`✓ Saved to: ${path.basename(outputPath)} (${ddl.length} bytes)`);
}

/**
 * Example 1: Simple User entity with JSON view
 */
function exampleSimpleUserEntity(): void {
  printHeader("Example 1: Simple User Entity");

  // Load schema
  const schemaPath = path.join(
    __dirname,
    "../test_schemas/user_schema.json"
  );
  console.log(`Loading schema: user_schema.json`);

  let schema: SchemaObject;
  try {
    schema = loadSchema(schemaPath);
  } catch {
    // Fallback: create in-memory test schema
    schema = createSimpleUserSchema();
  }

  console.log(`✓ Schema types: ${schema.types.map((t) => t.name).join(", ")}`);

  // Generate JSON view for OLTP workload
  console.log("\nGenerating tv_user (JSON materialized view)...");
  const options: GenerateTvOptions = {
    schema,
    entity: "User",
    view: "user",
    refreshStrategy: "trigger-based",
    includeCompositionViews: false,
    includeMonitoringFunctions: true,
  };

  const tvDdl = generateTvDdl(options);
  console.log(`✓ Generated ${tvDdl.length} bytes`);

  // Validate
  console.log("Validating DDL...");
  const errors = validateGeneratedDdl(tvDdl);
  if (errors.length > 0) {
    console.log(`⚠ Validation warnings: ${errors.length}`);
    errors.slice(0, 3).forEach((error) => console.log(`  - ${error}`));
  } else {
    console.log("✓ DDL validation passed");
  }

  // Save
  const outputPath = path.join(__dirname, "../output_user_view.sql");
  saveDdlFile(tvDdl, outputPath, "user_schema.json", "tv_user");

  // Show stats
  console.log("\nDDL Statistics:");
  console.log(
    `  - CREATE statements: ${tvDdl.match(/CREATE/gi)?.length || 0}`
  );
  console.log(
    `  - COMMENT statements: ${tvDdl.match(/COMMENT/gi)?.length || 0}`
  );
  console.log(`  - Lines of code: ${tvDdl.split("\n").length}`);
}

/**
 * Example 2: Entities with relationships
 */
function exampleRelatedEntities(): void {
  printHeader("Example 2: Entities with Relationships");

  const schema = createUserWithPostsSchema();
  console.log(
    `Loaded schema with types: ${schema.types.map((t) => t.name).join(", ")}`
  );

  // Generate for User
  console.log("\nGenerating tv_user_profile (with composition views)...");
  const tvUser = generateTvDdl({
    schema,
    entity: "User",
    view: "user_profile",
    refreshStrategy: "trigger-based",
    includeCompositionViews: true,
    includeMonitoringFunctions: true,
  });
  console.log(`✓ Generated ${tvUser.length} bytes`);

  // Generate for Post
  console.log("\nGenerating tv_post...");
  const tvPost = generateTvDdl({
    schema,
    entity: "Post",
    view: "post",
    refreshStrategy: "trigger-based",
    includeCompositionViews: false,
    includeMonitoringFunctions: true,
  });
  console.log(`✓ Generated ${tvPost.length} bytes`);

  // Save both
  const outputUserPath = path.join(__dirname, "../output_user_profile_view.sql");
  const outputPostPath = path.join(__dirname, "../output_post_view.sql");
  saveDdlFile(tvUser, outputUserPath, "user_with_posts.json", "tv_user_profile");
  saveDdlFile(tvPost, outputPostPath, "user_with_posts.json", "tv_post");

  console.log(`\n✓ Generated views with relationships`);
}

/**
 * Example 3: Arrow views for analytics
 */
function exampleArrowViews(): void {
  printHeader("Example 3: Arrow Views for Analytics");

  const schema = createOrderSchema();
  console.log(`Loaded schema with entity: Order`);

  // Generate Arrow view
  console.log("\nGenerating ta_order_analytics (Arrow columnar view)...");
  const taOrder = generateTaDdl({
    schema,
    entity: "Order",
    view: "order_analytics",
    refreshStrategy: "scheduled",
    includeMonitoringFunctions: true,
  });
  console.log(`✓ Generated ${taOrder.length} bytes`);

  // Save
  const outputPath = path.join(
    __dirname,
    "../output_order_analytics_arrow.sql"
  );
  saveDdlFile(taOrder, outputPath, "orders.json", "ta_order_analytics");

  console.log("\nArrow View Benefits:");
  console.log("  - Columnar storage for efficient analytics");
  console.log("  - Arrow Flight protocol support for streaming");
  console.log("  - Batch-based refresh for bulk operations");
  console.log("  - Metadata tracking for query optimization");
}

/**
 * Example 4: Automatic refresh strategy selection
 */
function exampleSmartRefreshStrategy(): void {
  printHeader("Example 4: Smart Refresh Strategy Selection");

  const schema = createSimpleUserSchema();

  // Workload 1: High-read OLTP
  console.log("Workload 1: High-read OLTP (user sessions)");
  const workload1Options: SuggestRefreshStrategyOptions = {
    writeVolumePerMinute: 100,
    latencyRequirementMs: 100,
    readVolumePerSecond: 50000 / 60, // Convert to per-second
  };
  const strategy1 = suggestRefreshStrategy(workload1Options);
  console.log(
    `  Writes/min: 100, Latency: 100ms, Reads/min: 50000 → ${strategy1}`
  );

  const ddl1 = generateTvDdl({
    schema,
    entity: "User",
    view: "user_session",
    refreshStrategy: strategy1 as "trigger-based" | "scheduled",
  });
  const output1 = path.join(
    __dirname,
    `../output_user_session_${strategy1}.sql`
  );
  saveDdlFile(ddl1, output1, "user.json", "tv_user_session");

  // Workload 2: Batch operations
  console.log("\nWorkload 2: Batch operations (daily reporting)");
  const workload2Options: SuggestRefreshStrategyOptions = {
    writeVolumePerMinute: 5000,
    latencyRequirementMs: 3600000, // 1 hour
    readVolumePerSecond: 100 / 60,
  };
  const strategy2 = suggestRefreshStrategy(workload2Options);
  console.log(
    `  Writes/min: 5000, Latency: 3600000ms, Reads/min: 100 → ${strategy2}`
  );

  const ddl2 = generateTvDdl({
    schema,
    entity: "User",
    view: "user_daily_report",
    refreshStrategy: strategy2 as "trigger-based" | "scheduled",
  });
  const output2 = path.join(
    __dirname,
    `../output_user_daily_${strategy2}.sql`
  );
  saveDdlFile(ddl2, output2, "user.json", "tv_user_daily_report");

  console.log("\n✓ Strategies selected based on workload characteristics");
}

/**
 * Example 5: Composition views
 */
function exampleCompositionViews(): void {
  printHeader("Example 5: Composition Views for Relationships");

  const schema = createUserWithPostsSchema();

  console.log("Generating composition views for User relationships...");
  const compositionSql = generateCompositionViews({
    schema,
    entity: "User",
    relationships: ["posts"],
  });

  console.log(`✓ Generated ${compositionSql.length} bytes`);
  console.log("Composition views enable:");
  console.log("  - Efficient nested entity loading");
  console.log("  - Batch composition operations");
  console.log("  - Reduced network round-trips");
}

/**
 * Example 6: Complete production deployment workflow
 */
function exampleProductionWorkflow(): void {
  printHeader("Example 6: Production Deployment Workflow");

  const schema = createOrderSchema();

  console.log("Deployment Steps:\n");

  // Step 1: Generate DDL
  console.log("1. Generate DDL for all views");
  const views = [
    { entity: "Order", view: "order", strategy: "trigger-based" as const },
    { entity: "Order", view: "order_analytics", strategy: "scheduled" as const },
  ];

  const ddlFiles: Array<[string, string]> = [];
  for (const { entity, view, strategy } of views) {
    const ddl = generateTvDdl({
      schema,
      entity,
      view,
      refreshStrategy: strategy,
    });
    const outputPath = path.join(__dirname, `../output_${view}_prod.sql`);
    saveDdlFile(ddl, outputPath, "orders.json", `tv_${view}`);
    ddlFiles.push([outputPath, ddl]);
  }

  // Step 2: Validate all DDL
  console.log("\n2. Validate generated DDL");
  for (const [outputPath, ddl] of ddlFiles) {
    const errors = validateGeneratedDdl(ddl);
    const status = errors.length === 0 ? "✓" : "⚠";
    console.log(
      `   ${status} ${path.basename(outputPath)}: ${errors.length} issues`
    );
  }

  // Step 3: Show deployment instructions
  console.log("\n3. Deployment Instructions");
  console.log("   # In PostgreSQL:");
  console.log("   # 1. Connect to target database");
  console.log("   # 2. Run: psql -d mydb -f output_order_prod.sql");
  console.log("   # 3. Monitor: SELECT * FROM v_staleness_order;");
  console.log("   # 4. Test: SELECT * FROM tv_order LIMIT 10;");

  console.log("\n✓ Production workflow complete");
}

/**
 * Helper: Create simple User schema
 */
function createSimpleUserSchema(): SchemaObject {
  return {
    types: [
      {
        name: "User",
        fields: [
          { name: "id", type: "Int", nullable: false },
          { name: "name", type: "String", nullable: false },
          { name: "email", type: "String", nullable: false },
          { name: "created_at", type: "DateTime", nullable: false },
        ],
        relationships: [],
      },
    ],
    queries: {},
    mutations: {},
  };
}

/**
 * Helper: Create User + Post schema with relationships
 */
function createUserWithPostsSchema(): SchemaObject {
  return {
    types: [
      {
        name: "User",
        fields: [
          { name: "id", type: "Int", nullable: false },
          { name: "name", type: "String", nullable: false },
          { name: "email", type: "String", nullable: false },
          { name: "created_at", type: "DateTime", nullable: false },
        ],
        relationships: [
          { name: "posts", target_entity: "Post", cardinality: "many" },
        ],
      },
      {
        name: "Post",
        fields: [
          { name: "id", type: "Int", nullable: false },
          { name: "title", type: "String", nullable: false },
          { name: "content", type: "String", nullable: false },
          { name: "user_id", type: "Int", nullable: false },
          { name: "created_at", type: "DateTime", nullable: false },
        ],
        relationships: [],
      },
    ],
    queries: {},
    mutations: {},
  };
}

/**
 * Helper: Create Order schema
 */
function createOrderSchema(): SchemaObject {
  return {
    types: [
      {
        name: "Order",
        fields: [
          { name: "id", type: "Int", nullable: false },
          { name: "order_number", type: "String", nullable: false },
          { name: "customer_id", type: "Int", nullable: false },
          { name: "status", type: "String", nullable: false },
          { name: "total_amount", type: "Int", nullable: false },
          { name: "created_at", type: "DateTime", nullable: false },
        ],
        relationships: [],
      },
    ],
    queries: {},
    mutations: {},
  };
}

/**
 * Main function: Run all examples
 */
async function main(): Promise<void> {
  console.log("=".repeat(80));
  console.log(
    " FraiseQL DDL Generation - Comprehensive TypeScript Examples"
  );
  console.log(" See: https://fraiseql.dev/docs/ddl-generation");
  console.log("=".repeat(80));

  try {
    exampleSimpleUserEntity();
    exampleRelatedEntities();
    exampleArrowViews();
    exampleSmartRefreshStrategy();
    exampleCompositionViews();
    exampleProductionWorkflow();

    printHeader("All Examples Completed Successfully", "=");

    console.log("Generated example outputs:");
    console.log("  ✓ Simple entity JSON view");
    console.log("  ✓ Related entities with composition views");
    console.log("  ✓ Arrow columnar views for analytics");
    console.log("  ✓ Smart refresh strategy selection");
    console.log("  ✓ Composition view generation");
    console.log("  ✓ Production deployment workflow");

    console.log("\nNext Steps:");
    console.log("  1. Review generated SQL files");
    console.log("  2. Test in development database: psql -f output_*.sql");
    console.log("  3. Adjust view names and refresh strategies as needed");
    console.log("  4. Deploy to production with pg_dump/pg_restore");
    console.log("");
  } catch (error) {
    console.error("\n✗ Error:", error instanceof Error ? error.message : error);
    process.exit(1);
  }
}

// Run if this is the main module
if (require.main === module) {
  main().catch((error) => {
    console.error("Unhandled error:", error);
    process.exit(1);
  });
}

export {
  exampleSimpleUserEntity,
  exampleRelatedEntities,
  exampleArrowViews,
  exampleSmartRefreshStrategy,
  exampleCompositionViews,
  exampleProductionWorkflow,
};
