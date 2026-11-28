"use strict";
// The reference server: graphql-js (the GraphQL reference implementation) serving the
// fixture's expected SDL over graphql-http, with resolvers over the same rows FraiseQL
// reads. The resolvers implement FraiseQL's filter/sort/pagination arguments directly
// on those rows, so a disagreement on `data` is either a FraiseQL defect or a gap in
// this model; the report shows both responses so a reader can tell which.

const http = require("node:http");
const { buildSchema, GraphQLError } = require("graphql");
const { createHandler } = require("graphql-http/lib/use/http");
const { Client } = require("pg");
const { applySpecifiedScalars } = require("./scalars.js");

const camel = (s) => s.replace(/_([a-z])/g, (_, c) => c.toUpperCase());

/** A stored row as the API presents it: camelCase keys, enum values in their GraphQL spelling. */
function present(row, typename) {
  const out = { __typename: typename };
  for (const [k, v] of Object.entries(row)) out[camel(k)] = v;
  if (out.role) out.role = out.role.toUpperCase();
  return out;
}

function order(a, b) {
  if (a === b) return 0;
  if (a === null || a === undefined) return 1;
  if (b === null || b === undefined) return -1;
  return a < b ? -1 : 1;
}

const same = (x, y) => JSON.stringify(x) === JSON.stringify(y);

function test(op, v, a) {
  switch (op) {
    case "eq":
      return same(v, a);
    case "neq":
      return v !== null && v !== undefined && !same(v, a);
    case "gt":
      return v != null && v > a;
    case "gte":
      return v != null && v >= a;
    case "lt":
      return v != null && v < a;
    case "lte":
      return v != null && v <= a;
    case "in":
      return a.some((x) => same(x, v));
    case "nin":
      return v != null && !a.some((x) => same(x, v));
    case "isnull":
      return (v == null) === a;
    case "is_not_null":
      return (v != null) === a;
    case "contains":
      return v != null && v.includes(a);
    case "icontains":
      return v != null && v.toLowerCase().includes(a.toLowerCase());
    case "startswith":
      return v != null && v.startsWith(a);
    case "endswith":
      return v != null && v.endsWith(a);
    default:
      throw new GraphQLError(`operator not modelled by the reference: ${op}`);
  }
}

function matches(row, where) {
  if (!where) return true;
  for (const [k, f] of Object.entries(where)) {
    if (f === null || f === undefined) continue;
    if (k === "_and") {
      if (!f.every((x) => matches(row, x))) return false;
    } else if (k === "_or") {
      if (!f.some((x) => matches(row, x))) return false;
    } else if (k === "_not") {
      if (matches(row, f)) return false;
    } else if (k === "author") {
      if (!row.author || !matches(row.author, f)) return false;
    } else {
      for (const [op, a] of Object.entries(f)) {
        if (a !== null && !test(op, row[k], a)) return false;
      }
    }
  }
  return true;
}

function list(rows, { where, orderBy, limit, offset }) {
  let r = rows.filter((x) => matches(x, where));
  if (orderBy && orderBy.length) {
    r = [...r].sort((a, b) => {
      for (const o of orderBy) {
        let c = order(a[o.field], b[o.field]);
        if ((o.direction || "ASC").startsWith("DESC")) c = -c;
        if (c) return c;
      }
      return 0;
    });
  }
  if (offset) r = r.slice(offset);
  if (limit !== undefined && limit !== null) r = r.slice(0, limit);
  return r;
}

/** Reads the fixture rows from `databaseUrl` and serves them on 127.0.0.1:`port` (0 = any). */
async function startReference({ sdl, databaseUrl, port = 0 }) {
  const db = new Client({ connectionString: databaseUrl });
  await db.connect();
  const authors = (await db.query("select data from v_author order by id")).rows.map((r) => present(r.data, "Author"));
  const posts = (await db.query("select data from v_post order by id")).rows.map((r) => {
    const p = present(r.data, "Post");
    if (p.author) p.author = present(p.author, "Author");
    return p;
  });
  await db.end();

  const schema = applySpecifiedScalars(buildSchema(sdl));
  schema.getType("Node").resolveType = (v) => v.__typename;
  const rootValue = {
    posts: (args) => list(posts, args),
    authors: (args) => list(authors, args),
    post: ({ id }) => posts.find((p) => p.id === id) || null,
    author: ({ id }) => authors.find((p) => p.id === id) || null,
    node: ({ id }) => {
      const [t, u] = Buffer.from(id, "base64").toString().split(":");
      if (t === "Post") return posts.find((p) => p.id === u) || null;
      if (t === "Author") return authors.find((p) => p.id === u) || null;
      return null;
    },
  };
  const handler = createHandler({ schema, rootValue });
  const server = http.createServer((req, res) => {
    if (req.url.startsWith("/graphql")) return handler(req, res);
    res.writeHead(404).end();
    return undefined;
  });
  await new Promise((resolve) => server.listen(port, "127.0.0.1", resolve));
  return { url: `http://127.0.0.1:${server.address().port}/graphql`, close: () => server.close() };
}

module.exports = { startReference };
