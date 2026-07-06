// Native Qonto sync — an `after:mutation:Invoice:*` function on the DURABLE path.
//
// Another workload migrated off the Python/FastAPI sidecar. Unlike the
// fire-and-forget scorer, this touches money, so it runs on
// the durable dispatch path: it is registered `re_runnable = false`, so a
// transient failure is retried with backoff and an exhausted one is dead-lettered.
// See docs/architecture/functions.md and ADR 0015.
//
// Money-path safety rests on ONE property: the idempotency key is *derived from
// the invoice*, never random. Every retry, replay, or backfill of the same
// invoice reuses the same key, so Qonto deduplicates server-side and the transfer
// is created at-most-once even though dispatch is at-least-once.
//
// Host surface used (all via `Deno.core.ops.fraiseql_*`, typed by the runtime):
//   - idempotency_token — host-provided per-dispatch idempotency key
//   - env_var       — read the Qonto API key (a secret, allowlisted by the host)
//   - http_request  — call Qonto (outbound HTTP, SSRF-allowlisted by the host)
//   - query         — record the Qonto reference back onto the invoice
//
// The function receives the mutated Invoice row as its argument.
//
// NOTE: this file uses the type-annotation-free TypeScript subset for brevity. The
// runtime strips TypeScript types before execution — see deal-scoring.ts for an
// annotated example and fraiseql-host.d.ts for the host-op types.
export default async (invoice) => {
  if (!invoice || !invoice.id) {
    throw new Error("Qonto sync requires an invoice id");
  }

  // Data-layer idempotency: a retry that ran after a successful write-back but
  // before its dispatch was acked sees the recorded reference and no-ops. This
  // is belt-and-braces with the API-level idempotency key below.
  if (invoice.qonto_reference) {
    return {
      skipped:    "already-synced",
      invoice_id: invoice.id,
      reference:  invoice.qonto_reference,
    };
  }

  const apiKey = Deno.core.ops.fraiseql_env_var("QONTO_API_KEY");
  if (!apiKey) {
    throw new Error("QONTO_API_KEY is not configured");
  }

  // Idempotency key — the money-path safety net. The host now provides a
  // per-dispatch token that is stable across retries and across a resume
  // (derived from the dispatch's identity, never wall-clock/random), so we no
  // longer have to hand-derive one. We fall back to an invoice-derived key on a
  // non-dispatched invocation (e.g. a one-shot `fraiseql query` run), where no
  // host token exists.
  //
  // Trade-off — both are valid: the host token dedups retries and redeliveries
  // of THIS dispatch. If you also need to dedup across DIFFERENT dispatches that
  // touch the same invoice (a manual backfill AND this auto-sync), make the
  // content-addressed `qonto-invoice-${invoice.id}` the primary instead — it
  // gives the strongest cross-dispatch guarantee at the cost of hand-derivation.
  const idempotencyKey =
    Deno.core.ops.fraiseql_idempotency_token() || `qonto-invoice-${invoice.id}`;

  const reqBody = Deno.core.encode(JSON.stringify({
    reference:    invoice.reference,
    amount_cents: invoice.amount_cents,
    currency:     invoice.currency || "EUR",
    counterparty: invoice.counterparty,
  }));
  const resp = await Deno.core.ops.fraiseql_http_request(
    "POST",
    "https://thirdparty.qonto.test/v2/external_transfers",
    [
      ["authorization", `Bearer ${apiKey}`],
      ["content-type", "application/json"],
      ["idempotency-key", idempotencyKey],
    ],
    reqBody,
  );

  // Fail loud on any non-2xx so the durable dispatcher — not this function —
  // decides what happens: a transient (5xx / timeout) failure is retried with
  // the SAME idempotency key; a client (4xx) failure is permanent and
  // dead-lettered. Never fabricate a success on the money path.
  if (resp.status < 200 || resp.status >= 300) {
    throw new Error(`Qonto sync failed: HTTP ${resp.status}`);
  }
  const created = JSON.parse(Deno.core.decode(resp.body));
  const reference = created.id;

  // Record the Qonto reference so a later retry short-circuits at the top.
  const writeBack = await Deno.core.ops.fraiseql_query(
    `mutation($id: ID!, $reference: String!) {
      recordQontoReference(invoiceId: $id, reference: $reference) {
        id
        qontoReference
      }
    }`,
    JSON.stringify({ id: invoice.id, reference }),
  );
  const written = JSON.parse(writeBack);

  return {
    invoice_id:      invoice.id,
    idempotency_key: idempotencyKey,
    reference,
    write_back_ok:   written.data !== undefined && written.data !== null,
  };
};
