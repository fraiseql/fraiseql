/**
 * FraiseQL HTTP client for executing GraphQL queries and mutations.
 */

import {
  FraiseQLError,
  GraphQLError,
  NetworkError,
  TimeoutError,
  AuthenticationError,
  RateLimitError,
} from './errors';
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

      let response: Response;
      try {
        response = await this.fetchFn(this.url, {
          method: 'POST',
          headers: await this.buildHeaders(),
          body,
          signal: controller.signal,
        });
      } catch (error) {
        if (
          error instanceof Error &&
          (error.name === 'AbortError' ||
            error.message.toLowerCase().includes('abort'))
        ) {
          throw new TimeoutError();
        }
        throw new NetworkError(
          error instanceof Error ? error.message : 'Network request failed',
          { cause: error }
        );
      } finally {
        clearTimeout(timer);
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

      if (!response.ok) {
        throw new NetworkError(
          `HTTP ${response.status}: ${response.statusText}`
        );
      }

      let json: GraphQLResponse;
      try {
        json = (await response.json()) as GraphQLResponse;
      } catch (error) {
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
  NetworkError,
  TimeoutError,
  AuthenticationError,
  RateLimitError,
};
export type { GraphQLErrorEntry } from './errors';
