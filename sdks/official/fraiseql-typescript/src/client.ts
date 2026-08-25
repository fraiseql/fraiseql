/**
 * FraiseQL HTTP client for executing GraphQL queries and mutations.
 */

import {
  FraiseQLError,
  GraphQLError,
  HttpStatusError,
  NetworkError,
  TimeoutError,
  AuthenticationError,
  RateLimitError,
} from './errors';

const HTTP_REQUEST_TIMEOUT = 408;
const HTTP_SERVER_ERROR_FLOOR = 500;
import type { HttpRetryConfig } from './http-retry';
import { executeWithRetry } from './http-retry';

export type { HttpRetryConfig };

/** Per-call overrides for a single `query()` or `mutate()`. */
export interface RequestOptions {
  /** Extra headers for this call only. They win over the client-level ones. */
  headers?: Record<string, string>;
  /**
   * Idempotency key for this call.
   *
   * A mutation carrying one executes at most once per key per tenant: a repeat
   * with the same body replays the stored response, a repeat with a different
   * body is a 409 conflict. `mutate()` generates one automatically when retry
   * is enabled, so pass this only to tie the key to your own unit of work.
   */
  idempotencyKey?: string;
}

export interface FraiseQLClientConfig {
  url: string;
  authorization?: string | (() => string | Promise<string>);
  timeoutMs?: number;
  retry?: HttpRetryConfig;
  headers?: Record<string, string>;
  fetch?: typeof fetch;
}

interface GraphQLResponse {
  data?: Record<string, unknown> | null;
  errors?: Array<{
    message: string;
    locations?: Array<{ line: number; column: number }>;
    path?: Array<string | number>;
    extensions?: Record<string, unknown>;
  }> | null;
}

/** A fresh idempotency key for one logical mutation. */
function newIdempotencyKey(): string {
  const webCrypto = globalThis.crypto as Crypto | undefined;
  if (typeof webCrypto?.randomUUID === 'function') {
    return webCrypto.randomUUID();
  }
  // Runtimes without WebCrypto: still unique per call, which is all the server
  // needs — it only compares keys for equality within a tenant.
  return `fraiseql-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 12)}`;
}

/** Whether a thrown value is the AbortController firing rather than a real fault. */
function isAbortError(error: unknown): boolean {
  return (
    error instanceof Error &&
    (error.name === 'AbortError' ||
      error.message.toLowerCase().includes('abort'))
  );
}

export class FraiseQLClient {
  private readonly url: string;
  private readonly authorization?: string | (() => string | Promise<string>);
  private readonly timeoutMs: number;
  private readonly retry: HttpRetryConfig;
  private readonly extraHeaders: Record<string, string>;
  private readonly fetchFn: typeof fetch;

  constructor(urlOrConfig: string | FraiseQLClientConfig) {
    const config: FraiseQLClientConfig =
      typeof urlOrConfig === 'string' ? { url: urlOrConfig } : urlOrConfig;

    this.url = config.url;
    this.authorization = config.authorization;
    this.timeoutMs = config.timeoutMs ?? 30_000;
    this.retry = config.retry ?? {};
    this.extraHeaders = config.headers ?? {};
    // Use globally available fetch by default; allow injection for tests
    this.fetchFn = config.fetch ?? globalThis.fetch.bind(globalThis);
  }

  private async resolveAuth(): Promise<string | undefined> {
    if (this.authorization === undefined) return undefined;
    if (typeof this.authorization === 'string') return this.authorization;
    return this.authorization();
  }

  private async buildHeaders(
    options?: RequestOptions,
    idempotencyKey?: string
  ): Promise<Record<string, string>> {
    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
      ...this.extraHeaders,
      ...options?.headers,
    };
    const auth = await this.resolveAuth();
    if (auth !== undefined) {
      headers['Authorization'] = auth;
    }
    if (idempotencyKey !== undefined) {
      headers['Idempotency-Key'] = idempotencyKey;
    }
    return headers;
  }

  /** Whether this client is configured to send a request more than once. */
  private retriesEnabled(): boolean {
    return (this.retry.maxAttempts ?? 1) > 1;
  }

  private async executeRequest(
    body: string,
    options?: RequestOptions,
    idempotencyKey?: string
  ): Promise<Record<string, unknown>> {
    return executeWithRetry(async () => {
      const controller = new AbortController();
      const timer = setTimeout(() => controller.abort(), this.timeoutMs);

      // The timer is cleared only once the body has been read. Clearing it
      // after `fetch` alone would bound the header round-trip and leave the
      // body read with no deadline at all, so a stalled chunked body would
      // hang the caller (#1077).
      try {
        let response: Response;
        try {
          response = await this.fetchFn(this.url, {
            method: 'POST',
            headers: await this.buildHeaders(options, idempotencyKey),
            body,
            signal: controller.signal,
          });
        } catch (error) {
          if (isAbortError(error)) {
            throw new TimeoutError();
          }
          throw new NetworkError(
            error instanceof Error ? error.message : 'Network request failed',
            { cause: error }
          );
        }

        if (response.status === 401 || response.status === 403) {
          throw new AuthenticationError(response.status as 401 | 403);
        }

        if (response.status === 429) {
          const retryAfterHeader = response.headers.get('Retry-After');
          const retryAfterMs = retryAfterHeader
            ? parseInt(retryAfterHeader, 10) * 1000
            : undefined;
          throw new RateLimitError(
            Number.isNaN(retryAfterMs) ? undefined : retryAfterMs
          );
        }

        if (response.status === HTTP_REQUEST_TIMEOUT) {
          throw new TimeoutError('Server reported a request timeout (HTTP 408)');
        }

        if (!response.ok) {
          // Only 5xx is transient enough to be worth another attempt. A 4xx
          // means the request itself was rejected, which ADR-0015 §3 treats as
          // permanent — retrying it just repeats a known-bad request (#1059).
          if (response.status >= HTTP_SERVER_ERROR_FLOOR) {
            throw new NetworkError(
              `HTTP ${response.status}: ${response.statusText}`
            );
          }
          throw new HttpStatusError(response.status, response.statusText);
        }

        let json: GraphQLResponse;
        try {
          json = (await response.json()) as GraphQLResponse;
        } catch (error) {
          // An abort during the body read is the deadline firing, not a
          // malformed payload; reporting it as NetworkError would both mislead
          // the caller and make it retryable.
          if (isAbortError(error)) {
            throw new TimeoutError();
          }
          throw new NetworkError('Failed to parse JSON response', {
            cause: error,
          });
        }

        // null/absent errors array means success — do NOT treat as error
        if (
          json.errors !== null &&
          json.errors !== undefined &&
          json.errors.length > 0
        ) {
          throw new GraphQLError(json.errors);
        }

        return (json.data as Record<string, unknown>) ?? {};
      } finally {
        clearTimeout(timer);
      }
    }, this.retry);
  }

  async query<T = Record<string, unknown>>(
    query: string,
    variables?: Record<string, unknown>,
    operationName?: string,
    options?: RequestOptions
  ): Promise<T> {
    const body = JSON.stringify({
      query,
      variables,
      ...(operationName && { operationName }),
    });
    // No generated key: repeating a read is already safe, and a key would make
    // the server store a response for something that does not need replaying.
    return this.executeRequest(
      body,
      options,
      options?.idempotencyKey
    ) as Promise<T>;
  }

  async mutate<T = Record<string, unknown>>(
    mutation: string,
    variables?: Record<string, unknown>,
    operationName?: string,
    options?: RequestOptions
  ): Promise<T> {
    const body = JSON.stringify({
      query: mutation,
      variables,
      ...(operationName && { operationName }),
    });
    // One key per *logical* call, shared by every attempt. Without it a retry
    // of a lost response re-executes the mutation and commits twice; with it
    // the server replays the first response instead (#1060).
    const idempotencyKey =
      options?.idempotencyKey ??
      (this.retriesEnabled() ? newIdempotencyKey() : undefined);
    return this.executeRequest(body, options, idempotencyKey) as Promise<T>;
  }
}

// Re-export errors for convenience
export {
  FraiseQLError,
  GraphQLError,
  HttpStatusError,
  NetworkError,
  TimeoutError,
  AuthenticationError,
  RateLimitError,
};
export type { GraphQLErrorEntry } from './errors';
