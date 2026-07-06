// Native reply-awareness — an `after:ingest:email` function.
//
// Stops an active outreach sequence when a human replies. This is the workload
// the poll-IMAP email adapter was built for. It runs on the durable dispatch path, so
// a transient failure is retried and an exhausted one is dead-lettered.
//
// The inbound email has already been normalized by the shared layer: the message
// is classified (human / out-of-office / bounce / challenge / auto-generated),
// threaded (Message-ID / In-Reply-To / References → thread_key), and deduplicated
// on the durable spine. This function only decides what to do with a reply.
//
// Host surface used (all via `Deno.core.ops.fraiseql_*`, typed by the runtime):
//   - query — look up the active sequence and stop it
//
// The function receives the normalized InboundMessage as its argument.
//
// NOTE: this file uses the type-annotation-free TypeScript subset for brevity. The
// runtime strips TypeScript types before execution — see deal-scoring.ts for an
// annotated example and fraiseql-host.d.ts for the host-op types.
export default async (message) => {
  // Reply-awareness and loop protection in one gate: only a human reply advances
  // the sequence. Bounces, out-of-office replies, challenges, and our own
  // auto-mail are still recorded on the durable spine, but must never stop a
  // sequence — and never trigger a response that could form a mail loop.
  if (message.classification !== "human") {
    return { ignored: message.classification, message: message.idempotency_key };
  }

  // The sender is the prospect; the thread key ties the reply to the sequence
  // that produced it.
  const sender = message.from;
  if (!sender) {
    return { ignored: "no-sender", message: message.idempotency_key };
  }
  const threadKey = message.thread_key === undefined ? null : message.thread_key;

  // Stop the active sequence for this prospect. The mutation is idempotent —
  // stopping an already-stopped sequence is a no-op — so at-least-once delivery
  // of the inbound message is safe to retry.
  const raw = await Deno.core.ops.fraiseql_query(
    `mutation($email: String!, $thread: String) {
      stopSequenceForReply(prospectEmail: $email, threadKey: $thread) {
        sequenceId
        stopped
      }
    }`,
    JSON.stringify({ email: sender, thread: threadKey }),
  );
  const result = JSON.parse(raw);
  const stopped =
    result.data !== undefined && result.data !== null
      ? result.data.stopSequenceForReply
      : null;

  return {
    stopped_for: sender,
    thread_key: threadKey,
    subject: message.subject,
    sequence: stopped,
  };
};
