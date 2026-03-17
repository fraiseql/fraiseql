/**
 * Retry logic for FraiseQL HTTP client operations.
 */

import type { FraiseQLError } from './errors';
import { NetworkError, TimeoutError } from './errors';

export interface HttpRetryConfig {
  maxAttempts?: number;
  baseDelayMs?: number;
  maxDelayMs?: number;
  jitter?: boolean;
  retryOn?: Array<new (...args: never[]) => FraiseQLError>;
  onRetry?: (attempt: number, error: FraiseQLError) => void;
}

export async function executeWithRetry<T>(
  fn: () => Promise<T>,
  config: HttpRetryConfig = {}
): Promise<T> {
  const {
    maxAttempts = 1,
    baseDelayMs = 1000,
    maxDelayMs = 30_000,
    jitter = true,
    retryOn = [NetworkError, TimeoutError],
    onRetry,
  } = config;

  let lastError: FraiseQLError | undefined;

  for (let attempt = 1; attempt <= maxAttempts; attempt++) {
    try {
      return await fn();
    } catch (error) {
      if (attempt === maxAttempts) throw error;

      const isRetryable = retryOn.some(
        (ErrorClass) => error instanceof ErrorClass
      );
      if (!isRetryable) throw error;

      lastError = error as FraiseQLError;
      onRetry?.(attempt, lastError);

      const delay = Math.min(
        baseDelayMs * Math.pow(2, attempt - 1),
        maxDelayMs
      );
      const actualDelay = jitter ? delay * (0.5 + Math.random() * 0.5) : delay;
      await new Promise((resolve) => setTimeout(resolve, actualDelay));
    }
  }

  // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
  throw lastError!;
}
