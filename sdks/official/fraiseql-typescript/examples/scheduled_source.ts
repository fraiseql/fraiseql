/**
 * Example: authoring a scheduled ingress Source (#573).
 *
 * A `Source` is the dual of an observer: instead of reacting to a database change
 * (egress), it runs on a cron schedule to pull from an external system and drive
 * results into the database via mutations (ingress), resuming from a durable cursor.
 *
 * This file is **authoring only** — it emits `sources[]` into the schema JSON. The
 * connector that actually runs (the Model B loop: cursor → HTTP → mutate → advance)
 * is a separate Deno function, `sources/poll_orders.connector.ts`, referenced here
 * by name.
 *
 * Usage:
 *   ts-node examples/scheduled_source.ts   # writes scheduled_source_schema.json
 *
 * Then:
 *   fraiseql-cli compile scheduled_source_schema.json
 *   fraiseql-server --schema scheduled_source_schema.compiled.json   # (needs --features sources)
 */

import { Type, Source, exportSchema } from "../src/index";
import type { ID } from "../src/index";

// The entity the connector upserts into.
@Type()
class Order {
  /** A customer order ingested from the upstream orders API. */
  id!: ID;
  total!: number;
}

// The source registrations live on a class; the method name is the source name.
class OrderSources {
  /**
   * Every 5 minutes, run the `pollOrders` connector to ingest new orders.
   *
   * `runAs` is the least-privilege ceiling the connector's mutations execute under
   * (#573): here it may only write orders. Omit `runAs` and the source is
   * fail-closed (its mutations are RLS/authz-denied) until you grant one.
   *
   * `function` defaults to the method name (`pollOrders`) — the Deno connector.
   */
  @Source({
    schedule: "*/5 * * * *",
    runAs: { roles: ["ingest_writer"], scopes: ["write:Order"] },
  })
  pollOrders() {
    /** Scheduled orders ingestion. */
  }
}

// Reference the class to trigger decorator registration.
void OrderSources;

if (require.main === module) {
  exportSchema("scheduled_source_schema.json");

  console.log("\n🎯 Source Summary:");
  console.log("   pollOrders → every 5 min, ingest orders as `ingest_writer`");
  console.log("\n✨ Next steps:");
  console.log("   1. fraiseql-cli compile scheduled_source_schema.json");
  console.log("   2. Ship the connector: sources/poll_orders.connector.ts");
  console.log("   3. fraiseql-server --schema ...compiled.json  (build --features sources)");
}
