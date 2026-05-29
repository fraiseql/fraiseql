/**
 * Pull-based FraiseQL change-log consumer (issue #149).
 *
 * Polls `entity_change_logs` with a keyset cursor, processes each batch, and
 * advances a per-consumer checkpoint — all over GraphQL. See
 * docs/guides/changelog-graphql.md.
 *
 * Env:
 *   FRAISEQL_URL    GraphQL endpoint (default http://localhost:8000/graphql)
 *   FRAISEQL_TOKEN  Bearer token carrying changelog_reader + changelog_writer
 *   CONSUMER_NAME   checkpoint transport_name (default "sidecar-1")
 *   BATCH_SIZE      rows per page (default 100)
 */

const URL = Deno?.env?.get?.("FRAISEQL_URL") ?? process.env.FRAISEQL_URL ??
  "http://localhost:8000/graphql";
const TOKEN = Deno?.env?.get?.("FRAISEQL_TOKEN") ?? process.env.FRAISEQL_TOKEN ?? "";
const CONSUMER = Deno?.env?.get?.("CONSUMER_NAME") ?? process.env.CONSUMER_NAME ?? "sidecar-1";
const BATCH = Number(Deno?.env?.get?.("BATCH_SIZE") ?? process.env.BATCH_SIZE ?? "100");

interface ChangeLogEvent {
  pk_entity_change_log: number;
  id: string;
  object_type: string;
  object_id: string;
  modification_type: string;
  object_data: unknown;
  created_at: string;
}

async function gql<T>(query: string, variables: Record<string, unknown>): Promise<T> {
  const res = await fetch(URL, {
    method: "POST",
    headers: {
      "content-type": "application/json",
      ...(TOKEN ? { authorization: `Bearer ${TOKEN}` } : {}),
    },
    body: JSON.stringify({ query, variables }),
  });
  const body = await res.json();
  if (body.errors) throw new Error(JSON.stringify(body.errors));
  return body.data as T;
}

const PAGE = /* GraphQL */ `
  query Page($cursor: Int!, $limit: Int!) {
    entity_change_logs(
      where:   { pk_entity_change_log: { gt: $cursor } }
      orderBy: { pk_entity_change_log: "ASC" }
      limit:   $limit
    ) {
      pk_entity_change_log
      id
      object_type
      object_id
      modification_type
      object_data
      created_at
    }
  }`;

const CURSOR = /* GraphQL */ `
  query Cursor($name: String!) {
    transport_checkpoint(transport_name: $name) { last_pk }
  }`;

const ADVANCE = /* GraphQL */ `
  mutation Advance($name: String!, $pk: Int!) {
    upsert_transport_checkpoint(transport_name: $name, last_pk: $pk) {
      transport_name
      last_pk
    }
  }`;

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** Replace with your real processing (index, score, notify, …). */
async function handle(event: ChangeLogEvent): Promise<void> {
  console.log(`#${event.pk_entity_change_log} ${event.modification_type} ${event.object_type}`);
}

async function main(): Promise<void> {
  console.log(`changelog-sidecar "${CONSUMER}" → ${URL}`);
  for (;;) {
    const cur = await gql<{ transport_checkpoint: { last_pk: number } | null }>(
      CURSOR,
      { name: CONSUMER },
    );
    const cursor = cur.transport_checkpoint?.last_pk ?? 0;

    const page = await gql<{ entity_change_logs: ChangeLogEvent[] }>(
      PAGE,
      { cursor, limit: BATCH },
    );
    const events = page.entity_change_logs;
    if (events.length === 0) {
      await sleep(1000);
      continue;
    }

    for (const event of events) await handle(event);

    const last = events[events.length - 1].pk_entity_change_log;
    await gql(ADVANCE, { name: CONSUMER, pk: last });
    console.log(`advanced checkpoint → ${last}`);
  }
}

main().catch((err) => {
  console.error(err);
  (globalThis as { process?: { exit: (n: number) => void } }).process?.exit?.(1);
});
