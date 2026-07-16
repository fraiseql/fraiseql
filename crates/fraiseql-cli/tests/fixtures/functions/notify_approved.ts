// Test fixture for `fraiseql functions invoke` (phase 08).
//
// An `after:mutation:Order:update` function gated on `status` → `approved`. It
// exercises the harness's host-op recording and idempotency-token injection: it
// reads the per-dispatch token, logs, and issues a query the harness mocks.
//
// The guest argument is the dispatch `data` — `{ event_kind, old, new }` — so the
// mutated row is `event.new`.
export default async (event: { event_kind: string; old: unknown; new: { id: string } }) => {
  const order = event.new;
  const token = Deno.core.ops.fraiseql_idempotency_token();
  Deno.core.ops.fraiseql_log(1, `notifying order ${order.id}`);
  const raw = await Deno.core.ops.fraiseql_query(
    `mutation { markNotified(id: "${order.id}") { id } }`,
    "{}",
  );
  return { notified: true, order_id: order.id, token, query: JSON.parse(raw) };
};
