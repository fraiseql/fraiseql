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

  private async buildHeaders(): Promise<Record<string, string>> {
    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
      ...this.extraHeaders,
    };
    const auth = await this.resolveAuth();
    if (auth !== undefined) {
      headers['Authorization'] = auth;
    }
    return headers;
  }

  private async executeRequest(
    body: string
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
            headers: await this.buildHeaders(),
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
    operationName?: string
  ): Promise<T> {
    const body = JSON.stringify({
      query,
      variables,
      ...(operationName && { operationName }),
    });
    return this.executeRequest(body) as Promise<T>;
  }

  async mutate<T = Record<string, unknown>>(
    mutation: string,
    variables?: Record<string, unknown>,
    operationName?: string
  ): Promise<T> {
    const body = JSON.stringify({
      query: mutation,
      variables,
      ...(operationName && { operationName }),
    });
    return this.executeRequest(body) as Promise<T>;
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
