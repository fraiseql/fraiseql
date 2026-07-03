// Native per-user follow-up sender — an `after:mutation:Deal:update` function
// that acts on the scorer's recommended next action (native-runtime Phase 05).
//
// This is the reference implementation of the banked per-user send constraint:
// a paired outbound email goes FROM the connected user's verified address, taken
// only from the host auth context, NEVER a shared or default mailbox. There is no
// path here to send from anything else, and a missing verified address fails loud
// rather than falling back to a default sender. It mirrors the Rust policy
// `fraiseql_functions::outbound::resolve_sender_identity`, which the Phase 06
// `send_email` host op will enforce structurally (host-owned `from`).
//
// Host surface used (all via `Deno.core.ops.fraiseql_*`, typed by the runtime):
//   - auth_context  — the connected user's verified sending identity (the `from`)
//   - env_var       — the mail-provider API key (a secret, allowlisted by host)
//   - http_request  — send via the provider (outbound HTTP, SSRF-allowlisted)
//
// The function receives the mutated Deal row as its argument.
//
// NOTE: the runtime executes JavaScript. This file is written in the
// type-annotation-free subset of TypeScript (valid JS *and* valid TS); full TS
// type-stripping transpilation is a tracked follow-up.
export default async (deal) => {
  // Only act when the scorer recommended a follow-up.
  if (deal.next_action !== "send_follow_up") {
    return { skipped: deal.next_action || "no-action", deal_id: deal.id };
  }

  // Per-user send: resolve the `from` from the connected user's verified address
  // in the host auth context. No verified address → refuse to send (fail loud);
  // never fall back to a shared or default mailbox.
  const auth = JSON.parse(Deno.core.ops.fraiseql_auth_context());
  const from =
    auth && typeof auth.email === "string" &&
    auth.email.indexOf("@") !== -1 && auth.email.indexOf(" ") === -1
      ? auth.email.trim()
      : null;
  if (!from) {
    throw new Error(
      "refusing to send follow-up: no verified per-user sending address in auth context",
    );
  }

  const contact = deal.contact_email;
  if (!contact) {
    return { skipped: "no-contact", deal_id: deal.id };
  }

  const apiKey = Deno.core.ops.fraiseql_env_var("MAIL_API_KEY");
  if (!apiKey) {
    throw new Error("MAIL_API_KEY is not configured");
  }

  const reqBody = Deno.core.encode(JSON.stringify({
    from, // host-owned; the connected user's verified address, never a shared box
    to: contact,
    subject: `Following up on ${deal.name || "our conversation"}`,
    text: `Hi — just following up on ${deal.name || "our last exchange"}. ` +
      `Happy to answer any questions.`,
  }));
  const resp = await Deno.core.ops.fraiseql_http_request(
    "POST",
    "https://mail.provider.test/v1/send",
    [
      ["authorization", `Bearer ${apiKey}`],
      ["content-type", "application/json"],
    ],
    reqBody,
  );
  if (resp.status < 200 || resp.status >= 300) {
    throw new Error(`follow-up send failed: HTTP ${resp.status}`);
  }
  const sent = JSON.parse(Deno.core.decode(resp.body));

  return {
    deal_id:    deal.id,
    sent_from:  from,
    sent_to:    contact,
    message_id: sent.id,
  };
};
