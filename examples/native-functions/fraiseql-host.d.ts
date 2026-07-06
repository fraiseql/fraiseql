// FraiseQL native-functions host surface — ambient TypeScript declarations for the
// in-process Deno runtime. Reference it from a function to get type-checking and
// editor autocomplete for the host operations:
//
//     /// <reference path="./fraiseql-host.d.ts" />
//
// The host operations are reached as `Deno.core.ops.fraiseql_*`; `Deno.core.encode`
// / `Deno.core.decode` bridge strings and byte buffers. The `from` on send is
// host-owned (resolved from the caller's identity), never guest-chosen.

interface FraiseqlHttpResponse {
  status: number;
  headers: Array<[string, string]>;
  body: Uint8Array;
}

interface FraiseqlHostOps {
  /** Execute a GraphQL query/mutation. `variables` is a JSON string; returns a JSON string. */
  fraiseql_query(graphql: string, variables: string): Promise<string>;
  /** Execute a raw SQL query. `params` is a JSON array string; returns a JSON array string. */
  fraiseql_sql_query(sql: string, params: string): Promise<string>;
  /** Make an outbound HTTP request (SSRF-allowlisted by the host). */
  fraiseql_http_request(
    method: string,
    url: string,
    headers: Array<[string, string]>,
    body: Uint8Array | null,
  ): Promise<FraiseqlHttpResponse>;
  /** Retrieve an object from storage. */
  fraiseql_storage_get(bucket: string, key: string): Promise<Uint8Array>;
  /** Store an object to storage. */
  fraiseql_storage_put(
    bucket: string,
    key: string,
    body: Uint8Array,
    contentType: string,
  ): Promise<void>;
  /** Send an email. `from` is host-owned; the request JSON carries only { to, subject, text?, html?, reply_to? }. */
  fraiseql_send_email(request: string): Promise<string>;
  /** The authenticated caller's context, as a JSON string. */
  fraiseql_auth_context(): string;
  /** Read a host-allowlisted environment variable, or null. */
  fraiseql_env_var(name: string): string | null;
  /** Per-dispatch idempotency token, or null on a non-durably-dispatched invocation. */
  fraiseql_idempotency_token(): string | null;
  /** Structured log. Levels: 0=debug, 1=info, 2=warn, 3=error. */
  fraiseql_log(level: number, message: string): void;
}

declare namespace Deno {
  namespace core {
    const ops: FraiseqlHostOps;
    /** Encode a string to UTF-8 bytes. */
    function encode(text: string): Uint8Array;
    /** Decode UTF-8 bytes to a string. */
    function decode(bytes: Uint8Array): string;
  }
}
