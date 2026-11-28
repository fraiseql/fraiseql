"use strict";
// Fixture database lifecycle, so the runner needs node and no PostgreSQL client tools.
//
//   node lib/db.js create <admin-url> <name>   prints the new database's URL
//   node lib/db.js drop   <admin-url> <name>
//   node lib/db.js wait   <admin-url>          waits up to 60s for the server to accept

const fs = require("node:fs");
const path = require("node:path");
const { Client } = require("pg");

const NAME = /^[a-z_][a-z0-9_]{0,62}$/;

function withDatabase(url, name) {
  const u = new URL(url);
  u.pathname = `/${name}`;
  return u.toString();
}

async function run(url, fn) {
  const c = new Client({ connectionString: url });
  await c.connect();
  try {
    return await fn(c);
  } finally {
    await c.end();
  }
}

async function main([cmd, admin, name]) {
  if (cmd === "wait") {
    for (let i = 0; i < 120; i++) {
      try {
        await run(admin, (c) => c.query("select 1"));
        return;
      } catch {
        await new Promise((r) => setTimeout(r, 500));
      }
    }
    throw new Error("database did not accept connections within 60s");
  }
  if (!NAME.test(name || "")) throw new Error(`bad database name: ${name}`);
  await run(admin, (c) => c.query(`drop database if exists ${name} with (force)`));
  if (cmd === "drop") return;
  if (cmd !== "create") throw new Error(`unknown command: ${cmd}`);
  await run(admin, (c) => c.query(`create database ${name}`));
  const url = withDatabase(admin, name);
  const sql = fs.readFileSync(path.join(__dirname, "..", "fixture", "data.sql"), "utf8");
  await run(url, (c) => c.query(sql));
  console.log(url);
}

main(process.argv.slice(2)).catch((e) => {
  console.error(`db.js: ${e.message}`);
  process.exit(2);
});
