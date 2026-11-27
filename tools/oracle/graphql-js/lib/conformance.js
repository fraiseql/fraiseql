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
const WebSocket = require("ws");
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

const SUBSCRIBE = "subscription { postChanged { id title } }";

/**
 * Probes by restriction id. `server` picks the instance run.sh starts for it: `main`
 * (the fixture), `limits` (a low complexity ceiling), `trusted` (trusted documents in
 * strict mode) or `subs` (the fixture serving the `postChanged` subscription).
 *
 * A probe sends `body` over POST by default; `raw` sends a body verbatim, `via` picks
 * another transport (`get`, `sse`, `ws`). `code` narrows the row's codes to the one this
 * probe must carry. `accepted: true` makes it a control: it must NOT be refused, so a
 * server refusing every shape cannot pass the row.
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
  "subscription-subset": [
    { server: "subs", via: "ws", accepted: true, body: { query: SUBSCRIBE } },
    { server: "subs", via: "ws", code: "SUBSCRIPTION_REFUSED", body: { query: "subscription { postChanged { id body } }" } },
    { server: "subs", code: "UNSUPPORTED_FEATURE", body: { query: SUBSCRIBE } },
    { server: "subs", via: "get", code: "UNSUPPORTED_FEATURE", body: { query: SUBSCRIBE } },
    { server: "subs", via: "sse", code: "UNSUPPORTED_FEATURE", body: { query: SUBSCRIBE } },
  ],
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

const parsed = (status, text) => {
  try {
    const body = JSON.parse(text);
    return { status, body: Array.isArray(body) ? { __array: true } : body };
  } catch {
    return { status, body: { __raw: text.slice(0, 200) } };
  }
};

async function get(url, p) {
  const r = await fetch(`${url}?${new URLSearchParams({ query: p.body.query })}`, {
    headers: { accept: "application/graphql-response+json, application/json" },
  });
  return parsed(r.status, await r.text());
}

/** POST asking for an event stream; reads for up to 2s and keeps the first event's payload. */
async function sse(url, p) {
  const ac = new AbortController();
  const timer = setTimeout(() => ac.abort(), 2000);
  let status = 0;
  let text = "";
  try {
    const r = await fetch(url, {
      method: "POST",
      headers: { "content-type": "application/json", accept: "text/event-stream" },
      body: JSON.stringify(p.body),
      signal: ac.signal,
    });
    status = r.status;
    if (!(r.headers.get("content-type") || "").startsWith("text/event-stream")) return parsed(status, await r.text());
    const reader = r.body.getReader();
    const dec = new TextDecoder();
    while (!/^data:.*\n/m.test(text)) {
      const { value, done } = await reader.read();
      if (done) break;
      text += dec.decode(value);
    }
    ac.abort();
  } catch (e) {
    if (e.name !== "AbortError") throw e;
  } finally {
    clearTimeout(timer);
  }
  const data = /^data:(.*)$/m.exec(text);
  return data ? parsed(status, data[1]) : { status, body: { __raw: "event stream with no event" } };
}

/**
 * One `graphql-transport-ws` subscribe on the server's `/ws`. An error frame becomes
 * `{ errors }`; a subscription still open after 1.5s, with no error, becomes `{ data }`.
 */
function ws(url, p) {
  const wsUrl = url.replace(/^http/, "ws").replace(/\/graphql$/, "/ws");
  return new Promise((resolve, reject) => {
    const sock = new WebSocket(wsUrl, "graphql-transport-ws");
    let done = false;
    const finish = (r) => {
      if (done) return;
      done = true;
      clearTimeout(timer);
      sock.terminate();
      resolve(r);
    };
    const timer = setTimeout(() => finish({ status: 101, body: { data: { subscribed: true } } }), 1500);
    sock.on("open", () => sock.send(JSON.stringify({ type: "connection_init" })));
    sock.on("message", (raw) => {
      const f = JSON.parse(raw.toString());
      if (f.type === "ping") sock.send(JSON.stringify({ type: "pong" }));
      else if (f.type === "connection_ack") sock.send(JSON.stringify({ id: "1", type: "subscribe", payload: p.body }));
      else if (f.type === "error") finish({ status: 101, body: { errors: f.payload } });
      else if (f.type === "complete") finish({ status: 101, body: { errors: [{ message: "completed without an event" }] } });
    });
    sock.on("close", (code, reason) => finish({ status: 101, body: { __raw: `socket closed ${code} ${reason}` } }));
    sock.on("error", (e) => (done ? undefined : reject(e)));
  });
}

const VIA = { get, sse, ws };

/**
 * The verdict on one probe's response: a finding line, or null when the response is what
 * the row and the probe require. Pure, so the self-test can feed it responses.
 */
function judge(row, i, p, r) {
  const label = `restriction ${row.id}#${i}${p.via ? ` (${p.via})` : ""}`;
  if (p.code && !row.codes.includes(p.code)) return `${label}: probe expects ${p.code}, which the row does not list`;
  if (p.accepted) {
    return errorsOf(r).length || r.body.__raw !== undefined
      ? `${label}: a shape inside the subset is refused (${JSON.stringify(r.body).slice(0, 200)})`
      : null;
  }
  const allowed = new Set(p.code ? [p.code] : row.codes);
  if (documentedRefusal(r, allowed)) return null;
  const seen = errorsOf(r).map((e) => (e && e.extensions && e.extensions.code) || (e && e.code ? `code:${e.code}` : "-"));
  const shape =
    r.body.__raw !== undefined
      ? "not a GraphQL response"
      : !errorsOf(r).length
        ? "answered, no errors"
        : hasData(r)
          ? "data with errors"
          : "refused";
  return `${label}: not refused with ${[...allowed].join("|")} (HTTP ${r.status}, ${shape}, codes ${seen.join(",") || "none"})`;
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
    for (const [i, p] of probes.entries()) {
      const url = servers[p.server];
      if (!url) throw new Error(`no ${p.server} server for restriction ${row.id}`);
      const r = p.raw ? await post(url, p) : p.via ? await VIA[p.via](url, p) : await send(url, p.body);
      const finding = judge(row, i, p, r);
      if (finding) findings.push(finding);
    }
  }
  return findings.sort();
}

module.exports = { readRestrictions, documentedRefusal, checkRestrictions, judge, PROBES, DOC };
