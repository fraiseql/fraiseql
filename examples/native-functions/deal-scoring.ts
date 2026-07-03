// Native LLM deal-scorer — an `after:mutation:Deal:update` function.
//
// This is the first workload migrated off the Python/FastAPI sidecar onto the
// in-process TypeScript runtime (native-runtime-migration, Phase 01/05). It runs
// on the fire-and-forget path: scoring is re-runnable and must never block or
// alter the mutation response.
//
// Host surface used (all via `Deno.core.ops.fraiseql_*`, typed by the runtime):
//   - env_var       — read the LLM API key (a secret, allowlisted by the host)
//   - http_request  — call the LLM (outbound HTTP, SSRF-allowlisted by the host)
//   - query         — write the score back with a GraphQL mutation
//
// The function receives the mutated Deal row as its argument.
//
// NOTE: the runtime executes JavaScript. This file is written in the
// type-annotation-free subset of TypeScript (valid JS *and* valid TS); full TS
// type-stripping transpilation is a tracked follow-up.
export default async (deal) => {
  // Idempotency: never overwrite a score a human set by hand. Re-running this
  // function (retry, replay, backfill) on a human-edited deal is a no-op.
  if (deal.score_source === "human") {
    return { skipped: "human-edited", deal_id: deal.id };
  }

  const apiKey = Deno.core.ops.fraiseql_env_var("LLM_API_KEY");
  if (!apiKey) {
    throw new Error("LLM_API_KEY is not configured");
  }

  // 1. Score the deal with an LLM.
  const prompt = `Rate this sales deal 0-100 and explain briefly:\n${JSON.stringify(deal)}`;
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
  const completion = JSON.parse(Deno.core.decode(resp.body));
  const score = completion.score;

  // 2. Write the score back onto the deal (marked ai-sourced, so a later human
  //    edit wins over any re-score).
  const writeBack = await Deno.core.ops.fraiseql_query(
    `mutation($id: ID!, $score: Int!) {
      updateDealScore(id: $id, score: $score, source: "ai") { id score }
    }`,
    JSON.stringify({ id: deal.id, score }),
  );
  const written = JSON.parse(writeBack);

  return {
    deal_id: deal.id,
    score,
    rationale: completion.rationale,
    write_back_ok: written.data !== undefined && written.data !== null,
  };
};
