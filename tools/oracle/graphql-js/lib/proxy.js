"use strict";
// A forwarding HTTP proxy that can rewrite chosen responses. The self-test puts it in
// front of FraiseQL to make the server under test misbehave in a known way, without a
// rebuild, and checks that the oracle reports exactly that.

const http = require("node:http");

/**
 * Forwards every request to `target` unchanged. When `rewrite(requestJson, responseJson)`
 * returns an object for a JSON POST, that object is answered instead (status 200).
 */
async function startProxy(target, rewrite) {
  const t = new URL(target);
  const server = http.createServer((req, res) => {
    const chunks = [];
    req.on("data", (c) => chunks.push(c));
    req.on("end", () => {
      const body = Buffer.concat(chunks);
      const headers = { ...req.headers, host: t.host };
      const up = http.request(
        { host: t.hostname, port: t.port, method: req.method, path: req.url, headers },
        (upRes) => {
          const out = [];
          upRes.on("data", (c) => out.push(c));
          upRes.on("end", () => {
            const raw = Buffer.concat(out);
            let replaced;
            if (req.method === "POST") {
              try {
                replaced = rewrite(JSON.parse(body.toString()), JSON.parse(raw.toString()));
              } catch {
                replaced = undefined;
              }
            }
            if (replaced !== undefined) {
              const text = JSON.stringify(replaced);
              res.writeHead(200, { "content-type": upRes.headers["content-type"] || "application/json" });
              res.end(text);
            } else {
              const h = { ...upRes.headers };
              delete h["transfer-encoding"];
              h["content-length"] = String(raw.length);
              res.writeHead(upRes.statusCode, h);
              res.end(raw);
            }
          });
        },
      );
      up.on("error", (e) => {
        res.writeHead(502).end(String(e));
      });
      up.end(body);
    });
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  return { url: `http://127.0.0.1:${server.address().port}${t.pathname}`, close: () => server.close() };
}

module.exports = { startProxy };
