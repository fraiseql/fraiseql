/**
 * Example: DDL Generation for Table-Backed Views
 *
 * This example demonstrates how to use the @fraiseql/views library to generate
 * SQL DDL for table-backed views (tv_* for JSON planes, ta_* for Arrow planes).
 *
 * Following FraiseQL's philosophy of explicit over implicit, DDL generation is a
 * tool developers call after deciding to use table-backed views.
 */

import {
  loadSchema,
  generateTvDdl,
  generateTaDdl,
  generateCompositionViews,
  suggestRefreshStrategy,
  validateGeneratedDdl,
  type SchemaObject,
} from "@fraiseql/views";
import * as fs from "fs";

/**
 * Example 1: Generate DDL for a table-backed JSON view (tv_*)
 *
 * Use case: You've decided to cache User entities as JSONB for fast GraphQL queries.
 * This example generates the complete DDL including table, indexes, and refresh logic.
 */
async function example1_GenerateTvDdl(): Promise<void> {
  console.log("\n=== Example 1: Generate tv_* (Table-backed JSON View) ===\n");

  // Step 1: Load your schema
  const schema = loadSchema("schema.json");

  // Step 2: Generate DDL for tv_user_profile
  const ddl = generateTvDdl({
    schema,
    entity: "User",
    view: "user_profile",
    refreshStrategy: "trigger-based", // Real-time via database triggers
    includeCompositionViews: true, // Include helper views for nested relationships
    includeMonitoringFunctions: true, // Include staleness-checking functions
  });

  // Step 3: Write to file
  fs.writeFileSync("tv_user_profile.sql", ddl);
  console.log("✅ Generated: tv_user_profile.sql");

  // Step 4: Validate before deploying
  const errors = validateGeneratedDdl(ddl);
  if (errors.length > 0) {
    console.error("Validation errors:");
    errors.forEach((e) => console.error(`  - ${e}`));
  } else {
    console.log("✅ Validation passed");
  }

  // Step 5: Review and deploy
  console.log("\nNext steps:");
  console.log("1. Review tv_user_profile.sql");
  console.log("2. Test in staging environment");
  console.log("3. Deploy to production: psql < tv_user_profile.sql");
}

/**
 * Example 2: Generate DDL for a table-backed Arrow view (ta_*)
 *
 * Use case: You need to export User data as Arrow for analytics/columnar storage.
 * This example generates Arrow table with columnar encoding.
 */
async function example2_GenerateTaDdl(): Promise<void> {
  console.log("\n=== Example 2: Generate ta_* (Table-backed Arrow View) ===\n");

  const schema = loadSchema("schema.json");

  const ddl = generateTaDdl({
    schema,
    entity: "User",
    view: "user_stats",
    refreshStrategy: "scheduled", // Batch refresh via pg_cron
    includeMonitoringFunctions: true,
  });

  fs.writeFileSync("ta_user_stats.sql", ddl);
  console.log("✅ Generated: ta_user_stats.sql");

  // Validate
  const errors = validateGeneratedDdl(ddl);
  if (errors.length === 0) {
    console.log("✅ Validation passed");
  }
}

/**
 * Example 3: Suggest refresh strategy based on workload
 *
 * Use case: You're not sure whether to use trigger-based or scheduled refresh.
 * This example shows how to get a recommendation.
 */
async function example3_SuggestStrategy(): Promise<void> {
  console.log("\n=== Example 3: Suggest Refresh Strategy ===\n");

  // Scenario 1: High-traffic e-commerce platform
  console.log("Scenario 1: High-traffic platform (1000 writes/min, 50 reads/sec, <500ms latency)");
  const strategy1 = suggestRefreshStrategy({
    writeVolumePerMinute: 1000,
    latencyRequirementMs: 500,
    readVolumePerSecond: 50,
  });
  console.log(`Suggested: ${strategy1}`);
  console.log("Reason: High write volume + strict latency requires real-time updates\n");

  // Scenario 2: Analytics dashboard (batch)
  console.log("Scenario 2: Analytics dashboard (50 writes/min, 5 reads/sec, 5000ms latency OK)");
  const strategy2 = suggestRefreshStrategy({
    writeVolumePerMinute: 50,
    latencyRequirementMs: 5000,
    readVolumePerSecond: 5,
  });
  console.log(`Suggested: ${strategy2}`);
  console.log("Reason: Low write volume + relaxed latency allows batch refresh\n");
}

/**
 * Example 4: Generate composition views for nested relationships
 *
 * Use case: Your User entities have nested relationships (posts, comments).
 * This example generates helper views for efficient composition.
 */
async function example4_CompositionViews(): Promise<void> {
  console.log("\n=== Example 4: Composition Views for Relationships ===\n");

  const schema = loadSchema("schema.json");

  const sql = generateCompositionViews({
    schema,
    entity: "User",
    relationships: ["posts", "comments", "followers"],
  });

  fs.writeFileSync("cv_user_relationships.sql", sql);
  console.log("✅ Generated: cv_user_relationships.sql");
  console.log("Generated composition views for:");
  console.log("  - cv_User_posts (efficiently loads related posts)");
  console.log("  - cv_User_comments (efficiently loads related comments)");
  console.log("  - cv_User_followers (efficiently loads related followers)");
  console.log("  - batch_compose_User (batch helper function)");
}

/**
 * Example 5: Complete workflow - from schema to deployed DDL
 *
 * This example shows the full lifecycle of creating a table-backed view.
 */
async function example5_CompleteWorkflow(): Promise<void> {
  console.log("\n=== Example 5: Complete Workflow ===\n");

  // Step 1: Read schema
  console.log("Step 1: Loading schema...");
  const schema = loadSchema("schema.json");
  console.log(`  Found ${schema.types.length} entities`);

  // Step 2: Developer decides to use table-backed view
  console.log("\nStep 2: Developer decides to use tv_order_summary");
  console.log("  (view selection guide)");

  // Step 3: Suggest refresh strategy
  console.log("\nStep 3: Getting refresh strategy recommendation...");
  const strategy = suggestRefreshStrategy({
    writeVolumePerMinute: 500,
    latencyRequirementMs: 2000,
    readVolumePerSecond: 100,
  });
  console.log(`  Recommended: ${strategy}`);

  // Step 4: Generate DDL
  console.log("\nStep 4: Generating DDL...");
  const ddl = generateTvDdl({
    schema,
    entity: "Order",
    view: "order_summary",
    refreshStrategy: strategy,
    includeCompositionViews: true,
    includeMonitoringFunctions: true,
  });

  // Step 5: Validate
  console.log("\nStep 5: Validating DDL...");
  const errors = validateGeneratedDdl(ddl);
  if (errors.length > 0) {
    console.error("  Errors found:");
    errors.forEach((e) => console.error(`    - ${e}`));
  } else {
    console.log("  ✅ Validation passed");
  }

  // Step 6: Write to file
  console.log("\nStep 6: Writing DDL to file...");
  fs.writeFileSync("tv_order_summary.sql", ddl);
  console.log("  ✅ Created: tv_order_summary.sql");

  // Step 7: Next steps
  console.log("\nStep 7: Next steps:");
  console.log("  1. Review tv_order_summary.sql in code review");
  console.log("  2. Test in staging: psql staging < tv_order_summary.sql");
  console.log("  3. Verify staleness detection: SELECT * FROM v_staleness_order_summary;");
  console.log("  4. Deploy to production: psql prod < tv_order_summary.sql");
  console.log("  5. Monitor with: SELECT check_staleness_order_summary();");
}

/**
 * Example 6: Integration with build pipeline
 *
 * Use case: Generate DDL as part of your build process.
 * This creates a views.sql file with all table-backed views.
 */
async function example6_BuildPipeline(): Promise<void> {
  console.log("\n=== Example 6: Build Pipeline Integration ===\n");

  const schema = loadSchema("schema.json");
  let allDdl = "-- ============================================================================\n";
  allDdl += "-- All table-backed views\n";
  allDdl += "-- Generated by @fraiseql/views\n";
  allDdl += `-- Generated at: ${new Date().toISOString()}\n`;
  allDdl += "-- ============================================================================\n\n";

  // Generate DDL for each view configuration
  const viewConfigs = [
    { entity: "User", view: "user_profile", strategy: "trigger-based" as const },
    { entity: "Order", view: "order_summary", strategy: "scheduled" as const },
    { entity: "Product", view: "product_catalog", strategy: "trigger-based" as const },
  ];

  for (const config of viewConfigs) {
    console.log(`Generating tv_${config.view}...`);
    const ddl = generateTvDdl({
      schema,
      entity: config.entity,
      view: config.view,
      refreshStrategy: config.strategy,
      includeMonitoringFunctions: true,
    });

    allDdl += `\n-- ============================================================================\n`;
    allDdl += `-- View: tv_${config.view}\n`;
    allDdl += `-- ============================================================================\n\n`;
    allDdl += ddl;
    allDdl += "\n\n";
  }

  // Write all views to single file
  fs.writeFileSync("views.sql", allDdl);
  console.log("\n✅ Generated: views.sql (ready to deploy)");
}

/**
 * Main: Run all examples
 */
async function main(): Promise<void> {
  console.log("╔════════════════════════════════════════════════════════════════╗");
  console.log("║  @fraiseql/views DDL Generation Examples                      ║");
  console.log("╚════════════════════════════════════════════════════════════════╝");

  try {
    // Note: These examples assume schema.json exists in current directory
    // For demo purposes, we'll show what each example does

    console.log("\n📚 Available Examples:");
    console.log("  1. Generate tv_* (Table-backed JSON View)");
    console.log("  2. Generate ta_* (Table-backed Arrow View)");
    console.log("  3. Suggest refresh strategy based on workload");
    console.log("  4. Generate composition views for relationships");
    console.log("  5. Complete workflow (schema → DDL → validation → deployment)");
    console.log("  6. Build pipeline integration (multi-view generation)");

    console.log("\n💡 To run these examples:");
    console.log("  1. Create a schema.json with your entities");
    console.log("  2. Run: npx ts-node examples/ddl_generation_example.ts");

    // Uncomment to run actual examples (requires schema.json):
    // await example1_GenerateTvDdl();
    // await example2_GenerateTaDdl();
    // await example3_SuggestStrategy();
    // await example4_CompositionViews();
    // await example5_CompleteWorkflow();
    // await example6_BuildPipeline();
  } catch (error) {
    console.error("Error:", error);
    process.exit(1);
  }
}

main();
