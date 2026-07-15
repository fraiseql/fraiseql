/**
 * Example Model B source connector (#573) — the runtime half.
 *
 * This is a **Deno function** (a scheduled `Source`'s `function`), NOT authoring
 * code: it runs inside FraiseQL's Deno runtime on the source's cron schedule, under
 * a single-firing lease, with a host bound to the source's durable cursor and its
 * `run_as` identity. Register it with `@Source` (see `../scheduled_source.ts`).
 *
 * The Model B loop the #573 issue shows:
 *   read cursor  ->  fetch the world (HTTP)  ->  mutate (fraiseql_query)  ->  advance cursor
 *
 * Contract:
 * - **At-least-once.** A crash before `ctx.advance` re-runs the whole window; make
 *   your mutation idempotent (natural keys / `ON CONFLICT`), because the framework
 *   guarantees at-least-once delivery, not exactly-once domain writes.
 * - **Durable cursor.** `ctx.cursor` is where the last successful run left off; only
 *   advance it after the writes for that window have committed.
 * - **Least privilege.** The mutation runs under the source's `run_as` ceiling; it
 *   can do nothing the ceiling doesn't grant (unset `run_as` ⇒ denied).
 */

// The host ops FraiseQL injects into the Deno runtime (see the host typings). Only
// the ones this connector uses are declared here.
declare const Deno: {
  core: {
    ops: {
      fraiseql_cursor_get(): Promise<string>;
      fraiseql_cursor_advance(valueJson: string): Promise<void>;
      fraiseql_query(graphql: string, variablesJson: string): Promise<string>;
      fraiseql_http_request(
        method: string,
        url: string,
        headers: Array<[string, string]>,
        body: Uint8Array | null
      ): Promise<{ status: number; headers: Array<[string, string]>; body: Uint8Array }>;
    };
  };
};

/**
 * A thin, ergonomic wrapper over the raw `fraiseql_*` host ops — copy this into a
 * shared module for your connectors. It hides the JSON string-encoding the ops use
 * and adds the multi-tenant `tenant` option.
 */
const ctx = {
  /** The value the source last advanced to, or `null` on the first run. */
  async cursor<T = unknown>(): Promise<T | null> {
    return JSON.parse(await Deno.core.ops.fraiseql_cursor_get()) as T | null;
  },

  /** Persist the new watermark. Call only after this window's writes committed. */
  async advance(value: unknown): Promise<void> {
    await Deno.core.ops.fraiseql_cursor_advance(JSON.stringify(value));
  },

  /**
   * Run a GraphQL query/mutation under the source's `run_as` identity.
   *
   * For a **multi-tenant** source, pass `{ tenant }` — it is delivered as the
   * reserved `__source_tenant` variable and re-scopes this one write to that tenant
   * (a source pinned to a tenant by `run_as` ignores it and cannot forge another).
   */
  async query<T = unknown>(
    graphql: string,
    variables: Record<string, unknown> = {},
    opts: { tenant?: string } = {}
  ): Promise<T> {
    const vars = opts.tenant ? { ...variables, __source_tenant: opts.tenant } : variables;
    return JSON.parse(await Deno.core.ops.fraiseql_query(graphql, JSON.stringify(vars))) as T;
  },

  /** A GET that returns parsed JSON, subject to the host's SSRF allowlist. */
  async fetchJson<T = unknown>(url: string): Promise<T> {
    const res = await Deno.core.ops.fraiseql_http_request("GET", url, [], null);
    return JSON.parse(new TextDecoder().decode(res.body)) as T;
  },
};

interface Order {
  id: string;
  total: number;
  tenant: string;
}

const UPSERT_ORDER = `
  mutation ($id: ID!, $total: Float!) {
    upsertOrder(id: $id, total: $total) { id }
  }
`;

/**
 * The connector entrypoint. The runtime invokes this once per scheduled fire.
 */
export default async function pollOrders(): Promise<void> {
  // 1. Resume from the durable cursor (an opaque, connector-defined watermark).
  const since = (await ctx.cursor<{ lastId: string }>())?.lastId ?? "0";

  // 2. Fetch the world. The host enforces the SSRF allowlist
  //    (`FRAISEQL_SOURCES_ALLOWED_DOMAINS`); an un-allowlisted host is rejected.
  const orders = await ctx.fetchJson<Order[]>(
    `https://orders.example.com/api/orders?since=${since}`
  );
  if (orders.length === 0) {
    return; // Nothing new — leave the cursor untouched.
  }

  // 3. Drive each record into the database via an idempotent mutation. For a
  //    multi-tenant source, scope each write to the record's tenant.
  for (const order of orders) {
    await ctx.query(UPSERT_ORDER, { id: order.id, total: order.total }, { tenant: order.tenant });
  }

  // 4. Advance the cursor past the last record — only now that the writes are in.
  await ctx.advance({ lastId: orders[orders.length - 1].id });
}
