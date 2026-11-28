"use strict";
// docs/graphql-conformance.md as the oracle reads it, and one probe per restriction.
//
// The document's "Restrictions" table is the allow-list: a refusal is documented when
// the response has no data and every error carries one of the listed codes in
// `extensions.code`. Each table row must have a probe here, and each probe must be
// refused with its row's code. So the list can name nothing the server does not
// refuse, and the corpus run counts every other refusal as a divergence.

const fs = require("node:fs");
const path = require("node:path");
const { send } = require("./compare.js");

const DOC = path.join(__dirname, "..", "..", "..", "..", "docs", "graphql-conformance.md");

/** The rows of the "## Restrictions" table: `[{ id, codes }]`. */
function readRestrictions(file = DOC) {
  const text = fs.readFileSync(file, "utf8");
  const start = text.indexOf("\n## Restrictions\n");
  if (start < 0) throw new Error(`${file}: no "## Restrictions" section`);
  const rows = [];
  for (const line of text.slice(start).split("\n").slice(2)) {
    if (line.startsWith("## ") || line.startsWith("### ")) break;
    const cells = line.split("|").map((c) => c.trim());
    if (cells.length < 5 || !/^`[a-z-]+`$/.test(cells[1])) continue;
    const codes = [...cells[3].matchAll(/`([A-Z_]+)`/g)].map((m) => m[1]);
    if (!codes.length) throw new Error(`${file}: restriction ${cells[1]} lists no code`);
    rows.push({ id: cells[1].slice(1, -1), codes });
  }
  if (!rows.length) throw new Error(`${file}: the Restrictions table has no rows`);
  return rows;
}

const errorsOf = (r) => (Array.isArray(r.body.errors) ? r.body.errors : []);
const hasData = (r) => r.body.data !== undefined && r.body.data !== null;
const codesOf = (r) => errorsOf(r).map((e) => (e && e.extensions && typeof e.extensions.code === "string" ? e.extensions.code : null));

/** The listed code a response refuses with, or null when it is not a documented refusal. */
function documentedRefusal(r, allowed) {
  const codes = codesOf(r);
  if (hasData(r) || !codes.length || !codes.every((c) => allowed.has(c))) return null;
  return [...new Set(codes)].sort().join(",");
}

const nest = (n) => `${"... on Post { ".repeat(n)}id${" }".repeat(n)}`;

/**
 * Probes by restriction id. `server` picks the instance run.sh starts for it: `main`
 * (the fixture), `limits` (a low complexity ceiling) or `trusted` (trusted documents in
 * strict mode). `raw` sends a body verbatim.
 */
const PROBES = {
  "depth-limit": [{ server: "main", body: { query: `{ posts(limit: 1) { ${nest(31)} } }` } }],
  "complexity-limit": [
    { server: "limits", body: { query: "{ posts { id title body views } authors { name email age } }" } },
  ],
  "alias-limit": [
    { server: "main", body: { query: `{ ${Array.from({ length: 31 }, (_, i) => `a${i}: __typename`).join(" ")} }` } },
  ],
  "page-size-cap": [{ server: "main", body: { query: "{ posts(limit: 1001) { id } }" } }],
  "non-negative-pagination": [
    { server: "main", body: { query: "{ posts(limit: -1) { id } }" } },
    { server: "main", body: { query: "{ posts(offset: -1) { id } }" } },
  ],
  "no-batching": [{ server: "main", raw: JSON.stringify([{ query: "{ __typename }" }, { query: "{ __typename }" }]) }],
  "trusted-documents": [
    { server: "trusted", body: { query: "{ __typename }" } },
    { server: "trusted", body: { documentId: "sha256:0000000000000000000000000000000000000000000000000000000000000000" } },
  ],
  // The fixture serves no subscription yet, so this entry has no probe: the oracle
  // reports it as unprobed rather than passing it.
  "subscription-subset": [],
};

async function post(url, p) {
  const r = await fetch(url, {
    method: "POST",
    headers: { "content-type": "application/json", accept: "application/graphql-response+json, application/json" },
    body: p.raw,
  });
  const text = await r.text();
  let body;
  try {
    body = JSON.parse(text);
  } catch {
    body = { __raw: text.slice(0, 200) };
  }
  return { status: r.status, body: Array.isArray(body) ? { __array: true } : body };
}

/**
 * Runs every probe; returns one line per row that is not refused as documented.
 * An empty list means the table and the server agree.
 */
async function checkRestrictions(rows, servers) {
  const findings = [];
  for (const id of Object.keys(PROBES)) {
    if (!rows.some((r) => r.id === id)) findings.push(`restriction ${id}: probed here but not in the document`);
  }
  for (const row of rows) {
    const probes = PROBES[row.id];
    if (!probes) {
      findings.push(`restriction ${row.id}: in the document, no probe`);
      continue;
    }
    if (!probes.length) {
      findings.push(`restriction ${row.id}: unprobed`);
      continue;
    }
    const allowed = new Set(row.codes);
    for (const [i, p] of probes.entries()) {
      const url = servers[p.server];
      if (!url) throw new Error(`no ${p.server} server for restriction ${row.id}`);
      const r = p.raw ? await post(url, p) : await send(url, p.body);
      if (!documentedRefusal(r, allowed)) {
        const seen = errorsOf(r).map((e) => (e && e.extensions && e.extensions.code) || (e && e.code ? `code:${e.code}` : "-"));
        const shape = r.body.__raw !== undefined ? "not a GraphQL response" : !errorsOf(r).length ? "answered, no errors" : hasData(r) ? "data with errors" : "refused";
        findings.push(
          `restriction ${row.id}#${i}: not refused with ${row.codes.join("|")} (HTTP ${r.status}, ${shape}, codes ${seen.join(",") || "none"})`,
        );
      }
    }
  }
  return findings.sort();
}

module.exports = { readRestrictions, documentedRefusal, checkRestrictions, PROBES, DOC };
