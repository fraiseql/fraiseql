"use strict";
// Sending one request to a GraphQL-over-HTTP endpoint, and comparing two responses.

const crypto = require("node:crypto");

/** POST `body` as JSON; returns `{ status, body }`, with an unparseable body kept as `{ __raw }`. */
async function send(url, body) {
  const r = await fetch(url, {
    method: "POST",
    headers: { "content-type": "application/json", accept: "application/graphql-response+json, application/json" },
    body: JSON.stringify(body),
  });
  const text = await r.text();
  let json;
  try {
    json = JSON.parse(text);
  } catch {
    json = { __raw: text };
  }
  return { status: r.status, body: json };
}

/** JSON with object keys sorted, so two values compare by content rather than key order. */
const canon = (v) =>
  JSON.stringify(v, (_, x) =>
    x && typeof x === "object" && !Array.isArray(x)
      ? Object.fromEntries(
          Object.keys(x)
            .sort()
            .map((k) => [k, x[k]]),
        )
      : x,
  );

const hasErrors = (r) => Boolean(r.body.errors && r.body.errors.length);

/**
 * The ways response `a` (FraiseQL) differs from `b` (graphql-js), as issue tags.
 * Messages are never compared: the spec leaves their wording to the implementation.
 */
function compare(a, b) {
  const issues = [];
  const ae = hasErrors(a);
  const be = hasErrors(b);
  if (ae !== be) issues.push(ae ? "FQ-only-error" : "REF-only-error");
  if ("data" in a.body !== "data" in b.body) issues.push("data-presence");
  if (canon(a.body.data) !== canon(b.body.data)) issues.push("data");
  else if (JSON.stringify(a.body.data) !== JSON.stringify(b.body.data)) issues.push("key-order");
  if (ae && be) {
    const sig = (errs) => errs.map((e) => ({ path: e.path ?? null, loc: e.locations ?? null }));
    const sa = sig(a.body.errors);
    const sb = sig(b.body.errors);
    const paths = (s) => [...new Set(s.map((x) => JSON.stringify(x.path)))].sort().join("|");
    if (paths(sa) !== paths(sb)) issues.push("err-paths");
    const locs = (s) => s.map((x) => canon(x.loc)).sort().join("|");
    if (locs(sa) !== locs(sb)) issues.push("err-locations");
    if (a.body.errors.length !== b.body.errors.length) issues.push("err-count");
  }
  for (const e of a.body.errors || []) {
    const extra = Object.keys(e).filter((k) => !["message", "locations", "path", "extensions"].includes(k));
    if (extra.length) {
      issues.push(`err-toplevel-keys:${extra.join(",")}`);
      break;
    }
  }
  return issues;
}

/** Stable identity of a request: the same document, variables and operation name hash alike everywhere. */
function fingerprint(c) {
  const material = canon({ q: c.query, v: c.variables ?? null, o: c.operationName ?? null });
  return crypto.createHash("sha256").update(material).digest("hex").slice(0, 16);
}

module.exports = { send, canon, compare, fingerprint, hasErrors };
