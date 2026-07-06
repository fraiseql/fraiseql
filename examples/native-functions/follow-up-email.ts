// Native per-user follow-up sender — an `after:mutation:Deal:update` function
// that acts on the scorer's recommended next action.
//
// This is the reference implementation of the banked per-user send constraint:
// a paired outbound email goes FROM the connected user's verified address, NEVER
// a shared or default mailbox. That is now enforced STRUCTURALLY by the
// `send_email` host op: the guest supplies only `to`/`subject`/body, and the host
// injects the `from` from the resolved sender identity (the connected user's
// verified address). There is no path here to send from anything else, and a
// missing verified identity fails the op loud rather than falling back.
//
// Host surface used (via `Deno.core.ops.fraiseql_*`, typed by the runtime):
//   - send_email  — host-owned `from`, per-connected-account SMTP transport
//
// The function receives the mutated Deal row as its argument.
//
// NOTE: this file uses the type-annotation-free TypeScript subset for brevity. The
// runtime strips TypeScript types before execution — see deal-scoring.ts for an
// annotated example and fraiseql-host.d.ts for the host-op types.
export default async (deal) => {
  // Only act when the scorer recommended a follow-up.
  if (deal.next_action !== "send_follow_up") {
    return { skipped: deal.next_action || "no-action", deal_id: deal.id };
  }

  const contact = deal.contact_email;
  if (!contact) {
    return { skipped: "no-contact", deal_id: deal.id };
  }

  // Send via the host op. The `from` is NOT ours to choose — the host resolves it
  // from the connected user's verified sending identity and injects it. A missing
  // identity or transport failure throws, so the durable dispatcher retries a
  // transient failure (5xx) and dead-letters a permanent one (a denied identity, a
  // bad recipient, a rejected relay).
  const raw = await Deno.core.ops.fraiseql_send_email(JSON.stringify({
    to: contact,
    subject: `Following up on ${deal.name || "our conversation"}`,
    text: `Hi — just following up on ${deal.name || "our last exchange"}. ` +
      `Happy to answer any questions.`,
  }));
  const sent = JSON.parse(raw);

  return {
    deal_id:    deal.id,
    sent_to:    contact,
    message_id: sent.message_id,
    accepted:   sent.accepted,
  };
};
