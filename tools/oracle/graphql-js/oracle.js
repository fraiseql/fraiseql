"use strict";
// graphql-js differential oracle.
//
// FraiseQL (at --server) and graphql-js (started here, over fixture/schema.graphql and
// the same database rows) answer the same documents: the targeted cases in
// lib/cases.js and a seeded random corpus from lib/fuzz.js. The oracle also diffs the
// two servers' introspection, runs the graphql-http audit against FraiseQL, and probes
// each restriction docs/graphql-conformance.md lists (lib/conformance.js).
//
// Each document ends in agreement, in a refusal the document lists, or in a divergence.
// Three metrics follow (docs/graphql-conformance.md):
//   - agreement on the supported subset: valid documents not refused as listed;
//   - invalid documents executed: graphql-js rejects them, FraiseQL answers;
//   - undocumented divergences: any other outcome that differs from graphql-js.
// Error-format differences (a key beside message/locations/path/extensions) are
// counted on their own.
//
// Every disagreement, listed refusal and restriction finding is compared with
// baseline.json, in both directions:
//   - a disagreement the baseline does not list is a regression: exit 1;
//   - a listed disagreement that no longer occurs is progress the baseline must
//     record: exit 1 until `--bless` rewrites it.
// So the baseline only ever shrinks toward full agreement, and every change to it is
// a reviewed diff.
//
// usage: node oracle.js --server <url> --limits-server <url> --trusted-server <url>
//                       --subscription-server <url> --database <postgres-url>
//                       [--baseline <path>] [--report <path>] [--bless] [--bless-sdl]
// run.sh starts the four servers: the fixture, the fixture with a low complexity
// ceiling, the fixture with trusted documents in strict mode, and the fixture serving
// a subscription.
// ORACLE_PRIVATE_MODULE may name a module, kept outside the repository, exporting
// `{ cases, mutations }`: extra targeted cases, and replacements for fuzz mutation
// slots by name. Such a run is measured against its own --baseline.
// Exit codes: 0 agreement with the baseline, 1 the gate failed, 2 the harness failed.

const fs = require("node:fs");
const path = require("node:path");
const { parseArgs } = require("node:util");
const { buildClientSchema, buildSchema, parse, printSchema, validate } = require("graphql");
const { auditServer } = require("graphql-http");
const { compare, fingerprint, hasErrors, send } = require("./lib/compare.js");
const { generate, MUTATIONS } = require("./lib/fuzz.js");
const { introspectionDiff, QUERY } = require("./lib/introspection.js");
const { startReference } = require("./lib/reference.js");
const { startProxy } = require("./lib/proxy.js");
const { applySpecifiedScalars } = require("./lib/scalars.js");
const { checkRestrictions, documentedRefusal, readRestrictions } = require("./lib/conformance.js");
const publicCases = require("./lib/cases.js");

const SEEDS = [11, 12];
const FUZZ_PER_SEED = 4000;
const WORKERS = 16;
// Regressions printed in full; the report file holds the rest.
const SHOWN = 25;
// The spec does not fix how many errors a failure reports.
const IGNORED = new Set(["err-count"]);
// An error-format difference, counted apart from the outcome.
const isFormat = (issue) => issue.startsWith("err-toplevel-keys");
const GATED = ["disagreements", "introspection", "audit", "restrictions"];

const here = __dirname;
const SDL_PATH = path.join(here, "fixture", "schema.graphql");

const { values: opt } = parseArgs({
  options: {
    server: { type: "string" },
    "limits-server": { type: "string" },
    "trusted-server": { type: "string" },
    "subscription-server": { type: "string" },
    database: { type: "string" },
    baseline: { type: "string", default: path.join(here, "baseline.json") },
    report: { type: "string" },
    bless: { type: "boolean", default: false },
    "bless-sdl": { type: "boolean", default: false },
    selftest: { type: "boolean", default: false },
  },
});

function loadPrivate() {
  const p = process.env.ORACLE_PRIVATE_MODULE;
  if (!p) return { cases: [], mutations: MUTATIONS, label: "public" };
  const m = require(path.resolve(p));
  const mutations = MUTATIONS.map((slot) => (m.mutations && m.mutations[slot.name]) || slot);
  return { cases: m.cases || [], mutations, label: `private:${path.basename(p)}` };
}

async function blessSdl(server) {
  const r = await send(server, { query: QUERY });
  if (!r.body.data) throw new Error(`introspection failed: ${JSON.stringify(r.body).slice(0, 500)}`);
  fs.writeFileSync(SDL_PATH, `${printSchema(buildClientSchema(r.body.data))}\n`);
  console.log(`wrote ${path.relative(process.cwd(), SDL_PATH)} from the server's introspection`);
}

async function runAll(cases, fqUrl, refUrl, schema, allowed) {
  const results = new Array(cases.length);
  let next = 0;
  async function worker() {
    while (next < cases.length) {
      const i = next++;
      const c = cases[i];
      const body = { query: c.query };
      if (c.variables !== undefined) body.variables = c.variables;
      if (c.operationName !== undefined) body.operationName = c.operationName;
      const [fq, ref] = await Promise.all([send(fqUrl, body), send(refUrl, body)]);
      let valid;
      try {
        valid = validate(schema, parse(c.query)).length === 0;
      } catch {
        valid = false;
      }
      const issues = compare(fq, ref).filter((x) => !IGNORED.has(x.split(":")[0]));
      const refusal = documentedRefusal(fq, allowed);
      const outcome = refusal ? [] : issues.filter((x) => !isFormat(x));
      const format = issues.filter(isFormat);
      results[i] = { case: c, fp: fingerprint(c), valid, issues, outcome, format, refusal, fq, ref };
    }
  }
  await Promise.all(Array.from({ length: WORKERS }, worker));
  return results;
}

async function audit(url) {
  const results = await auditServer({ url });
  return results
    .filter((r) => r.status === "warn" || r.status === "error")
    .map((r) => `${r.status.toUpperCase()} ${r.id} ${r.name}`)
    .sort();
}

function setDiff(now, before) {
  const b = new Set(before);
  const n = new Set(now);
  return { added: [...n].filter((x) => !b.has(x)), removed: [...b].filter((x) => !n.has(x)) };
}

const pct = (a, b) => (b ? `${((100 * a) / b).toFixed(1)}%` : "n/a");
const clip = (v, n = 400) => JSON.stringify(v).slice(0, n);

function showCase(r) {
  const c = r.case;
  console.log(`  ${c.name} [${r.fp}] ${r.valid ? "valid" : "invalid"}: ${r.refusal ? `refused:${r.refusal}` : r.issues.join(",")}`);
  console.log(`    query: ${c.query}`);
  if (c.variables !== undefined) console.log(`    variables: ${clip(c.variables)}`);
  if (c.operationName !== undefined) console.log(`    operationName: ${clip(c.operationName)}`);
  console.log(`    FQ  (${r.fq.status}): ${clip(r.fq.body)}`);
  console.log(`    REF (${r.ref.status}): ${clip(r.ref.body)}`);
}

/** The key a result is gated under, or null when it agrees with graphql-js outright. */
function keyOf(r) {
  if (r.refusal) return `${r.fp} refused:${r.refusal}`;
  return r.issues.length ? `${r.fp} ${r.issues.join(",")}` : null;
}

/** Runs every document against both servers; returns the measured state and the gated results by key. */
async function measure(servers, database, priv) {
  const sdl = fs.readFileSync(SDL_PATH, "utf8");
  const schema = applySpecifiedScalars(buildSchema(sdl));
  const rows = readRestrictions();
  const allowed = new Set(rows.flatMap((r) => r.codes));
  const targeted = [...publicCases, ...priv.cases].map((c) => ({ ...c, name: `case:${c.name}` }));
  const fuzz = SEEDS.flatMap((s) => generate(FUZZ_PER_SEED, s, priv.mutations));

  const ref = await startReference({ sdl, databaseUrl: database });
  let results;
  let intro;
  let auditLines;
  let restrictions;
  try {
    results = await runAll([...targeted, ...fuzz], servers.main, ref.url, schema, allowed);
    intro = await introspectionDiff(servers.main, ref.url);
    auditLines = await audit(servers.main);
    restrictions = await checkRestrictions(rows, servers);
  } finally {
    ref.close();
  }

  const valid = results.filter((r) => r.valid);
  const supported = valid.filter((r) => !r.refusal);
  const invalid = results.filter((r) => !r.valid);
  const metrics = {
    supported_agree: supported.filter((r) => !r.outcome.length).length,
    supported_total: supported.length,
    invalid_executed: invalid.filter((r) => !hasErrors(r.fq)).length,
    invalid_total: invalid.length,
    undocumented_divergences: results.filter((r) => r.outcome.length).length,
    documented_refusals: results.filter((r) => r.refusal).length,
    error_format_divergences: results.filter((r) => r.format.length).length,
    introspection_diff_lines: intro.length,
    audit_warn: auditLines.filter((l) => l.startsWith("WARN")).length,
    audit_error: auditLines.filter((l) => l.startsWith("ERROR")).length,
    restriction_findings: restrictions.length,
  };
  const failing = {};
  for (const r of results) {
    const k = keyOf(r);
    if (k) failing[k] = r;
  }
  const current = {
    corpus: { label: priv.label, seeds: SEEDS, fuzz_per_seed: FUZZ_PER_SEED, targeted: targeted.length },
    metrics,
    disagreements: Object.keys(failing).sort(),
    introspection: intro,
    audit: auditLines,
    restrictions,
  };
  return { current, failing };
}

const METRIC_LINES = [
  ["agreement on the supported subset", "supported_agree", "supported_total", +1],
  ["invalid documents executed", "invalid_executed", "invalid_total", -1],
  ["undocumented divergences", "undocumented_divergences", null, -1],
  ["documented refusals", "documented_refusals", null, 0],
  ["error-format divergences", "error_format_divergences", null, -1],
  ["introspection diff lines", "introspection_diff_lines", null, -1],
  ["graphql-http audit warnings", "audit_warn", null, -1],
  ["graphql-http audit errors", "audit_error", null, -1],
  ["restriction findings", "restriction_findings", null, -1],
];

/** One line per metric; with a baseline, the change from it and whether that is better. */
function printMetrics(label, m, base) {
  console.log(`graphql-js oracle (${label})`);
  for (const [name, k, total, good] of METRIC_LINES) {
    let v = `${m[k]}`;
    if (total) v += `/${m[total]} (${pct(m[k], m[total])})`;
    let delta = "";
    const b = base && base.metrics ? base.metrics[k] : undefined;
    if (b !== undefined && b !== m[k]) {
      const d = m[k] - b;
      const verdict = good === 0 ? "" : Math.sign(d) === good ? " better" : " worse";
      delta = `   ${d > 0 ? "+" : ""}${d} vs baseline${verdict}`;
    }
    console.log(`  ${`${name}:`.padEnd(36)}${v}${delta}`);
  }
}

/**
 * Compares a measurement with a baseline. Pure: returns what it found, prints nothing.
 * `ok` holds only when the two are equal in every direction.
 */
function gate(current, base) {
  if (canonCorpus(base.corpus) !== canonCorpus(current.corpus)) {
    return { ok: false, corpusChanged: true, regressions: {}, progress: {} };
  }
  const regressions = {};
  const progress = {};
  for (const key of GATED) {
    const d = setDiff(current[key], base[key] || []);
    regressions[key] = d.added;
    progress[key] = d.removed;
  }
  const count = (o) => Object.values(o).reduce((n, xs) => n + xs.length, 0);
  return { ok: count(regressions) === 0 && count(progress) === 0, corpusChanged: false, regressions, progress, count };
}

function printGate(g, current, base, failing) {
  if (g.corpusChanged) {
    console.log(`FAIL: the corpus changed (${clip(base.corpus)} -> ${clip(current.corpus)}); re-bless the baseline`);
    return;
  }
  const r = g.regressions;
  const p = g.progress;
  if (r.disagreements.length) {
    console.log(`\nREGRESSION: ${r.disagreements.length} disagreement(s) not in the baseline:`);
    for (const k of r.disagreements.slice(0, SHOWN)) showCase(failing[k]);
    if (r.disagreements.length > SHOWN) {
      console.log(`  ... and ${r.disagreements.length - SHOWN} more; --report <file> writes every one with both responses`);
    }
  }
  for (const l of r.introspection) console.log(`REGRESSION: introspection: ${l}`);
  for (const l of r.audit) console.log(`REGRESSION: audit: ${l}`);
  for (const l of r.restrictions) console.log(`REGRESSION: ${l}`);
  const progress = g.count(p);
  if (progress) {
    console.log(`\nPROGRESS not yet recorded: ${progress} baseline entr${progress === 1 ? "y" : "ies"} no longer occur:`);
    for (const k of p.disagreements) console.log(`  ${k}`);
    for (const l of p.introspection) console.log(`  introspection: ${l}`);
    for (const l of p.audit) console.log(`  audit: ${l}`);
    for (const l of p.restrictions) console.log(`  ${l}`);
    console.log("Record it: rerun with --bless and commit baseline.json with the change.");
  }
  if (g.ok) console.log("\nOK: agreement matches the baseline exactly");
  else console.log(`\nFAIL: ${g.count(r)} regression(s), ${progress} unrecorded improvement(s)`);
}

const readBaseline = (p) => JSON.parse(fs.readFileSync(p, "utf8"));

/**
 * Proves the gate goes RED. On a server that matches the baseline it checks that:
 *  - each gate site (disagreements, introspection, audit, in both directions, and the
 *    corpus) fails when the baseline is changed by exactly one entry, naming that entry;
 *  - a server that starts executing one class of invalid document (simulated by a proxy
 *    that answers those requests with data) is reported, and only that class is;
 *  - a server that returns one wrong value is reported, and only those documents are.
 */
async function selftest(servers, database, priv, base) {
  const checks = [];
  const check = (name, ok, detail = "") => {
    checks.push(ok);
    console.log(`${ok ? "PASS" : "FAIL"} ${name}${detail ? `: ${detail}` : ""}`);
  };
  const only = (g, key, expected) =>
    !g.ok &&
    !g.corpusChanged &&
    ["regressions", "progress"].every((side) =>
      GATED.every((k) =>
        side === key[0] && k === key[1] ? canon(g[side][k]) === canon(expected) : g[side][k].length === 0,
      ),
    );
  const { canon } = require("./lib/compare.js");

  const { current } = await measure(servers, database, priv);
  const g0 = gate(current, base);
  if (!g0.ok) throw new Error("the self-test needs a server that matches the baseline; run the oracle first");
  check("unchanged server matches the baseline", g0.ok);

  for (const k of GATED) {
    const entry = base[k][0];
    if (entry !== undefined) {
      const dropped = { ...base, [k]: base[k].slice(1) };
      check(`${k}: an entry missing from the baseline is a regression`, only(gate(current, dropped), ["regressions", k], [entry]));
    } else check(`${k}: the baseline has an entry to remove`, false, "empty");
    const fake = `selftest-${k}-entry`;
    const extra = { ...base, [k]: [...base[k], fake] };
    check(`${k}: a baseline entry that no longer occurs is unrecorded progress`, only(gate(current, extra), ["progress", k], [fake]));
  }
  const otherCorpus = { ...base, corpus: { ...base.corpus, seeds: [...base.corpus.seeds, 1] } };
  check("a changed corpus fails", gate(current, otherCorpus).corpusChanged === true);

  async function viaProxy(name, rewrite, marker) {
    const proxy = await startProxy(servers.main, rewrite);
    let m;
    try {
      m = await measure({ ...servers, main: proxy.url }, database, priv);
    } finally {
      proxy.close();
    }
    const g = gate(m.current, base);
    const regs = g.regressions.disagreements || [];
    const allMarked = regs.every((k) => marker(m.failing[k]));
    // A document that already disagreed and now disagrees differently is listed under
    // both regressions (new issue set) and progress (old one); only those may appear
    // as progress.
    const fp = (k) => k.split(" ")[0];
    const regFps = new Set(regs.map(fp));
    const nothingElse =
      !g.corpusChanged &&
      ["introspection", "audit", "restrictions"].every(
        (k) => g.regressions[k].length === 0 && g.progress[k].length === 0,
      ) &&
      g.progress.disagreements.every((k) => regFps.has(fp(k)));
    check(name, !g.ok && regs.length > 0 && allMarked && nothingElse, `${regs.length} regression(s)`);
  }

  // A server that stops rejecting unknown directives (§ 5.7.1).
  await viaProxy(
    "a server executing documents with an unknown directive is reported, and nothing else",
    (req, res) => (typeof req.query === "string" && req.query.includes(" @zz") && hasErrors({ body: res }) ? { data: {} } : undefined),
    (r) => r.case.query.includes(" @zz") && r.issues.includes("REF-only-error"),
  );
  // A server that answers one wrong value.
  await viaProxy(
    "a server returning one wrong value is reported, and nothing else",
    (req, res) => {
      const text = JSON.stringify(res);
      return res.data && text.includes('"Post 1"') ? JSON.parse(text.replaceAll('"Post 1"', '"Post 1 (altered)"')) : undefined;
    },
    (r) => JSON.stringify(r.fq.body).includes("Post 1 (altered)") && r.issues.includes("data"),
  );

  // A server that refuses the depth and page-size restrictions as documented: the two
  // probe findings must go, the corpus documents it refuses that way must count as
  // documented refusals, and nothing else may move. Without this, a probe that can
  // never pass would look the same as a server that never complies.
  {
    const asDocumented = (req, res) => {
      const msg = JSON.stringify(res.errors || []);
      if (res.data || !/exceeds maximum depth|exceeds the maximum page size/.test(msg)) return undefined;
      return { errors: res.errors.map((e) => ({ message: e.message, extensions: { code: "LIMIT_EXCEEDED" } })) };
    };
    const proxy = await startProxy(servers.main, asDocumented);
    let m;
    try {
      m = await measure({ ...servers, main: proxy.url }, database, priv);
    } finally {
      proxy.close();
    }
    const g = gate(m.current, base);
    const gone = g.progress.restrictions;
    const probesOk =
      gone.length === 2 &&
      gone.some((l) => l.startsWith("restriction depth-limit#0:")) &&
      gone.some((l) => l.startsWith("restriction page-size-cap#0:")) &&
      g.regressions.restrictions.length === 0;
    const newKeys = g.regressions.disagreements;
    const corpusOk =
      newKeys.length > 0 &&
      newKeys.every((k) => k.endsWith(" refused:LIMIT_EXCEEDED")) &&
      m.current.metrics.documented_refusals === newKeys.length &&
      g.progress.disagreements.every((k) => newKeys.some((n) => n.split(" ")[0] === k.split(" ")[0])) &&
      ["introspection", "audit"].every((k) => g.regressions[k].length === 0 && g.progress[k].length === 0);
    check(
      "a server refusing as documented clears exactly those probes, and its refusals count as documented",
      probesOk && corpusOk,
      `${gone.length} probe finding(s) cleared, ${newKeys.length} documented refusal(s)`,
    );
  }

  // A server whose errors carry their code in `extensions.code`, as the error format
  // requires, but whose codes are not listed restrictions: error-format divergences
  // must fall, and not one of its refusals may count as documented.
  {
    const codesInExtensions = (req, res) => {
      if (!Array.isArray(res.errors) || !res.errors.some((e) => e.code !== undefined)) return undefined;
      const errors = res.errors.map(({ code, ...e }) => ({ ...e, extensions: { ...(e.extensions || {}), code } }));
      return "data" in res ? { data: res.data, errors } : { errors };
    };
    const proxy = await startProxy(servers.main, codesInExtensions);
    let m;
    try {
      m = await measure({ ...servers, main: proxy.url }, database, priv);
    } finally {
      proxy.close();
    }
    const mm = m.current.metrics;
    check(
      "codes moved into extensions.code end the format divergences, and an unlisted code documents nothing",
      mm.error_format_divergences < current.metrics.error_format_divergences && mm.documented_refusals === 0,
      `format ${current.metrics.error_format_divergences} -> ${mm.error_format_divergences}, documented ${mm.documented_refusals}`,
    );
  }

  // The subscription control probe can fail: against a server serving no subscription,
  // the shape inside the subset is refused, and the probe reports it.
  {
    const findings = await checkRestrictions(readRestrictions(), { ...servers, subs: servers.main });
    const control = findings.filter((l) => l.startsWith("restriction subscription-subset#0: a shape inside the subset is refused"));
    check("a refused subscription inside the subset is reported", control.length === 1, `${control.length} control finding(s)`);
  }

  const failed = checks.filter((ok) => !ok).length;
  console.log(failed ? `\nSELFTEST FAIL: ${failed} of ${checks.length} checks` : `\nSELFTEST OK: ${checks.length} checks`);
  return failed ? 1 : 0;
}

async function main() {
  if (!opt.server) throw new Error("--server is required");
  if (opt["bless-sdl"]) return blessSdl(opt.server);
  if (!opt.database) throw new Error("--database is required");
  for (const k of ["limits-server", "trusted-server", "subscription-server"]) if (!opt[k]) throw new Error(`--${k} is required`);
  const servers = { main: opt.server, limits: opt["limits-server"], trusted: opt["trusted-server"], subs: opt["subscription-server"] };
  const priv = loadPrivate();
  if (opt.selftest) return selftest(servers, opt.database, priv, readBaseline(opt.baseline));

  const { current, failing } = await measure(servers, opt.database, priv);
  const prior = fs.existsSync(opt.baseline) ? readBaseline(opt.baseline) : null;
  printMetrics(priv.label, current.metrics, prior);
  if (opt.report) {
    const cases = Object.values(failing);
    fs.writeFileSync(opt.report, `${JSON.stringify({ ...current, cases }, null, 1)}\n`);
  }
  if (opt.bless) {
    fs.writeFileSync(opt.baseline, `${JSON.stringify(current, null, 1)}\n`);
    console.log(`blessed ${path.relative(process.cwd(), opt.baseline)}`);
    return 0;
  }
  if (!fs.existsSync(opt.baseline)) {
    console.log(`FAIL: no baseline at ${opt.baseline}; run with --bless to record one`);
    return 1;
  }
  const base = prior;
  const g = gate(current, base);
  printGate(g, current, base, failing);
  return g.ok ? 0 : 1;
}

const canonCorpus = (c) => JSON.stringify(c ?? null);

main().then(
  (code) => process.exit(code ?? 0),
  (err) => {
    console.error(`oracle harness error: ${err && err.stack ? err.stack : err}`);
    process.exit(2);
  },
);
