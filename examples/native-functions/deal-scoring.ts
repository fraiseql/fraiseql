/// <reference path="./fraiseql-host.d.ts" />
//
// Native LLM deal-scorer + next-action — an `after:mutation:Deal:update` function.
//
// This is the first workload migrated off the Python/FastAPI sidecar onto the
// in-process TypeScript runtime. It runs
// on the fire-and-forget path: scoring is re-runnable and must never block or
// alter the mutation response.
//
// It does two things the sidecar used to: score the deal, and recommend the
// rep's next action (the value the follow-up-email.ts function acts on). Both are
// written back in one mutation.
//
// Host surface used (all via `Deno.core.ops.fraiseql_*`, typed by fraiseql-host.d.ts):
//   - env_var       — read the LLM API key (a secret, allowlisted by the host)
//   - http_request  — call the LLM (outbound HTTP, SSRF-allowlisted by the host)
//   - query         — write the score + next-action back with a GraphQL mutation
//
// The function receives the mutated Deal row as its argument.
//
// This file is ordinary TypeScript — interfaces, type annotations, an `as`
// assertion — which the runtime type-strips to JavaScript before execution.

// The mutated Deal row this function receives (only the fields it reads are typed).
interface Deal {
  id: string;
  score_source?: string;
}

// The LLM's structured reply.
interface ScoreCompletion {
  score: number;
  next_action: string;
  rationale: string;
}

type ScoreResult =
  | { skipped: string; deal_id: string }
  | {
      deal_id: string;
      score: number;
      next_action: string;
      rationale: string;
      write_back_ok: boolean;
    };

// The next-action vocabulary the model may return. An unrecognised action is
// coerced to "wait" so a downstream actor (follow-up-email.ts) never dispatches
// on a value it does not understand.
const NEXT_ACTIONS: string[] = ["send_follow_up", "wait", "escalate", "mark_lost"];

export default async (deal: Deal): Promise<ScoreResult> => {
  // Idempotency: never overwrite a score/action a human set by hand. Re-running
  // this function (retry, replay, backfill) on a human-edited deal is a no-op.
  if (deal.score_source === "human") {
    return { skipped: "human-edited", deal_id: deal.id };
  }

  const apiKey: string | null = Deno.core.ops.fraiseql_env_var("LLM_API_KEY");
  if (!apiKey) {
    throw new Error("LLM_API_KEY is not configured");
  }

  // 1. Score the deal and ask for the recommended next action.
  const prompt =
    `Rate this sales deal 0-100, recommend one next action ` +
    `(${NEXT_ACTIONS.join(", ")}), and explain briefly:\n${JSON.stringify(deal)}`;
  const reqBody = Deno.core.encode(JSON.stringify({
    model: "claude-opus-4-8",
    input: prompt,
  }));
  const resp = await Deno.core.ops.fraiseql_http_request(
    "POST",
    "https://api.llm.test/v1/score",
    [
      ["authorization", `Bearer ${apiKey}`],
      ["content-type", "application/json"],
    ],
    reqBody,
  );
  if (resp.status !== 200) {
    throw new Error(`LLM scoring failed: HTTP ${resp.status}`);
  }
  const completion = JSON.parse(Deno.core.decode(resp.body)) as ScoreCompletion;
  const score: number = completion.score;
  const nextAction: string = NEXT_ACTIONS.indexOf(completion.next_action) === -1
    ? "wait"
    : completion.next_action;

  // 2. Write the score + next action back onto the deal (marked ai-sourced, so a
  //    later human edit wins over any re-score).
  const writeBack = await Deno.core.ops.fraiseql_query(
    `mutation($id: ID!, $score: Int!, $nextAction: String!) {
      updateDealScore(id: $id, score: $score, nextAction: $nextAction, source: "ai") {
        id
        score
        nextAction
      }
    }`,
    JSON.stringify({ id: deal.id, score, nextAction }),
  );
  const written = JSON.parse(writeBack) as { data?: unknown };

  return {
    deal_id: deal.id,
    score,
    next_action: nextAction,
    rationale: completion.rationale,
    write_back_ok: written.data !== undefined && written.data !== null,
  };
};
