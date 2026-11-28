"use strict";
// The full introspection query against both servers, diffed per schema key, type and
// directive. Each difference is one line, so a baseline can hold them as a set.

const { getIntrospectionQuery } = require("graphql");
const { send, canon } = require("./compare.js");

const QUERY = getIntrospectionQuery({
  descriptions: true,
  specifiedByUrl: true,
  directiveIsRepeatable: true,
  schemaDescription: true,
  inputValueDeprecation: true,
});

const clip = (v) => JSON.stringify(v).slice(0, 300);

async function introspectionDiff(fqUrl, refUrl) {
  const [a, b] = await Promise.all([send(fqUrl, { query: QUERY }), send(refUrl, { query: QUERY })]);
  if (!b.body.data) throw new Error(`reference introspection failed: ${clip(b.body)}`);
  if (!a.body.data) return [`FQ introspection failed: ${clip(a.body.errors ?? a.body)}`];
  const A = a.body.data.__schema;
  const B = b.body.data.__schema;
  const lines = [];
  for (const k of Object.keys(B)) {
    if (k !== "types" && k !== "directives" && canon(A[k]) !== canon(B[k])) {
      lines.push(`schema.${k}: FQ ${clip(A[k])} RF ${clip(B[k])}`);
    }
  }
  const byName = (xs) => Object.fromEntries((xs || []).map((t) => [t.name, t]));
  const ta = byName(A.types);
  const tb = byName(B.types);
  for (const n of [...new Set([...Object.keys(ta), ...Object.keys(tb)])].sort()) {
    if (!ta[n]) lines.push(`type ${n}: missing in FQ`);
    else if (!tb[n]) lines.push(`type ${n}: extra in FQ`);
    else {
      for (const k of Object.keys(tb[n])) {
        if (canon(ta[n][k]) !== canon(tb[n][k])) lines.push(`type ${n}.${k}: FQ ${clip(ta[n][k])} RF ${clip(tb[n][k])}`);
      }
    }
  }
  const da = byName(A.directives);
  const db = byName(B.directives);
  for (const n of [...new Set([...Object.keys(da), ...Object.keys(db)])].sort()) {
    if (canon(da[n]) !== canon(db[n])) lines.push(`directive @${n}: FQ ${clip(da[n] ?? null)} RF ${clip(db[n] ?? null)}`);
  }
  return lines;
}

module.exports = { introspectionDiff, QUERY };
