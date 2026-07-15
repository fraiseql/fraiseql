/**
 * Source authoring API for FraiseQL scheduled ingress (#573).
 *
 * A source is the dual of an observer: on a cron schedule it fires a Deno connector
 * that pulls from an external system and drives results into the database via
 * mutations, resuming from a durable cursor. NO runtime behavior — only used for
 * schema compilation.
 *
 * @example
 * ```typescript
 * import { Source } from "fraiseql";
 *
 * class Sources {
 *   // Every 5 minutes, run the `pollOrders` connector as `ingest_writer`.
 *   @Source({ schedule: "*\/5 * * * *", runAs: { roles: ["ingest_writer"] } })
 *   pollOrders() {}
 * }
 * ```
 */

import { SchemaRegistry, SourceRunAs } from "./registry";

/**
 * Configuration for the @Source decorator.
 */
interface SourceConfig {
  /** POSIX cron expression the source polls on (e.g. `"*\/5 * * * *"`). */
  schedule: string;
  /** The bound Deno connector function name; defaults to the decorated method name. */
  function?: string;
  /** Distinct durable-cursor name; defaults to the source name. */
  cursor?: string;
  /** Whether the source is scheduled. Default `true`. */
  enabled?: boolean;
  /** Least-privilege authority ceiling for the source's mutations (#573 D6). */
  runAs?: SourceRunAs;
  /** Connector-specific options, opaque to the framework. */
  options?: Record<string, unknown>;
}

/**
 * Method decorator to register a scheduled ingress source with the schema registry.
 *
 * The decorated method's name is the source name (the durable-cursor row and
 * advisory-lease key); its `function` defaults to the same name.
 *
 * @param config - Source configuration
 * @returns Method decorator
 */
export function Source(config: SourceConfig) {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any -- legacy method decorator target
  return function (_target: any, propertyKey: string, _descriptor: PropertyDescriptor): void {
    SchemaRegistry.registerSource(
      propertyKey,
      config.schedule,
      config.function ?? propertyKey,
      config.cursor,
      config.enabled ?? true,
      config.runAs,
      config.options
    );
  };
}
