/**
 * Error hierarchy for FraiseQL HTTP client.
 */

export class FraiseQLError extends Error {
  constructor(message: string, options?: ErrorOptions) {
    super(message, options);
    this.name = 'FraiseQLError';
  }
}

export interface GraphQLErrorEntry {
  message: string;
  locations?: Array<{ line: number; column: number }>;
  path?: Array<string | number>;
  extensions?: Record<string, unknown>;
}

export class GraphQLError extends FraiseQLError {
  readonly errors: GraphQLErrorEntry[];
  constructor(errors: GraphQLErrorEntry[]) {
    super(errors[0]?.message ?? 'GraphQL error');
    this.name = 'GraphQLError';
    this.errors = errors;
  }
}

export class NetworkError extends FraiseQLError {
  constructor(message: string, options?: ErrorOptions) {
    super(message, options);
    this.name = 'NetworkError';
  }
}

export class TimeoutError extends NetworkError {
  constructor(message = 'Request timed out') {
    super(message);
    this.name = 'TimeoutError';
  }
}

export class AuthenticationError extends FraiseQLError {
  readonly statusCode: 401 | 403;
  constructor(statusCode: 401 | 403) {
    super(`Authentication failed (HTTP ${statusCode})`);
    this.name = 'AuthenticationError';
    this.statusCode = statusCode;
  }
}

export class RateLimitError extends FraiseQLError {
  readonly retryAfterMs?: number;
  constructor(retryAfterMs?: number) {
    super('Rate limit exceeded');
    this.name = 'RateLimitError';
    this.retryAfterMs = retryAfterMs;
  }
}

/**
 * A non-2xx response that is permanent rather than transient.
 *
 * Covers the 4xx statuses not classified more specifically (401/403 →
 * {@link AuthenticationError}, 408 → {@link TimeoutError}, 429 →
 * {@link RateLimitError}). Per ADR-0015 §3 a 4xx means the request itself was
 * rejected, so — unlike {@link NetworkError} — it is never retried.
 */
export class HttpStatusError extends FraiseQLError {
  readonly status: number;
  constructor(status: number, statusText?: string) {
    super(`HTTP ${status}${statusText ? `: ${statusText}` : ''}`);
    this.name = 'HttpStatusError';
    this.status = status;
  }
}
