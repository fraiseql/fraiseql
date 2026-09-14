/**
 * Serverless function authoring API (#1325).
 *
 * A function is an out-of-band handler the server dispatches on a trigger — after a
 * committed mutation, before one, on a cron schedule, or on an inbound event — and
 * runs in a WASM or Deno sandbox. NO runtime behavior here; the runtime is Rust.
 *
 * The decorated member's name is the function name **and the module file stem**: the
 * server loads `<module_dir>/<name>.<ext>`, so it is carried verbatim rather than
 * recased like every other name this SDK emits.
 *
 * `moduleDir` and `dlqStore` are deliberately not options — they are deployment
 * settings, owned by `[functions]` in `fraiseql.toml`
 * (`docs/architecture/config-vs-settings.md`).
 *
 * @example
 * ```typescript
 * import { FraiseFunction } from "fraiseql";
 *
 * class Functions {
 *   // Runs functions/notify_approved.ts when an order transitions to approved.
 *   @FraiseFunction({
 *     trigger: "after:mutation:Order:update",
 *     when: [{ field: "status", changed_to: "approved" }],
 *   })
 *   notify_approved() {}
 * }
 * ```
 */

import { decoratedMemberName } from "./decorator-name";
import { FunctionPredicate, SchemaRegistry, SourceRunAs } from "./registry";

/**
 * Configuration for the `@FraiseFunction` decorator.
 */
interface FunctionConfig {
  /**
   * Trigger string, e.g. `"after:mutation:Order:update"`,
   * `"before:mutation:placeOrder"`, `"cron:0 * * * *"`.
   *
   * `after:mutation` matches the mutation's **return type**, not its name.
   */
  trigger: string;
  /** Which sandbox runs the module. Default `"Deno"`. */
  runtime?: "Deno" | "Wasm";
  /** Timeout override; defaults to 500ms for `before:mutation` and 5s otherwise. */
  timeoutMs?: number;
  /** Authority ceiling for the function's `fraiseql_query` writes (#594). Absent ⇒ fail-closed. */
  runAs?: SourceRunAs;
  /** Predicates (#597) evaluated on the row images before firing. */
  when?: FunctionPredicate[];
  /** Opt out of durable dispatch for work that is safe to re-run (ADR 0015). */
  reRunnable?: boolean;
  /** Per-function retry policy for durable dispatch. */
  retry?: Record<string, unknown>;
}

/**
 * Method decorator registering a serverless function with the schema registry.
 *
 * Named `FraiseFunction` rather than `Function` because `Function` is a global type
 * in TypeScript: a decorator with that name shadows it inside any module importing
 * this one, and the resulting errors point everywhere except here.
 *
 * @param config - Function configuration
 * @returns Method decorator
 */
export function FraiseFunction(config: FunctionConfig) {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any -- method decorator target
  return function (_target: any, propertyKeyOrContext: unknown, _descriptor?: PropertyDescriptor): void {
    const name = decoratedMemberName(propertyKeyOrContext, "FraiseFunction");
    SchemaRegistry.registerFunction({
      name,
      trigger: config.trigger,
      runtime: config.runtime ?? "Deno",
      ...(config.timeoutMs !== undefined ? { timeout_ms: config.timeoutMs } : {}),
      ...(config.runAs !== undefined ? { run_as: config.runAs } : {}),
      ...(config.when !== undefined ? { when: config.when } : {}),
      ...(config.reRunnable !== undefined ? { re_runnable: config.reRunnable } : {}),
      ...(config.retry !== undefined ? { retry: config.retry } : {}),
    });
  };
}
