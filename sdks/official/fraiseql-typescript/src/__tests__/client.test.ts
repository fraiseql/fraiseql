import { vi } from 'vitest';
import { FraiseQLClient } from '../client';
import {
  GraphQLError,
  HttpStatusError,
  NetworkError,
  TimeoutError,
  AuthenticationError,
  RateLimitError,
} from '../errors';

function makeFetch(response: {
  status?: number;
  ok?: boolean;
  statusText?: string;
  body?: unknown;
  headers?: Record<string, string>;
}): ReturnType<typeof vi.fn> {
  const status = response.status ?? 200;
  return vi.fn().mockResolvedValue({
    status,
    statusText: response.statusText ?? 'OK',
    ok: response.ok ?? (status >= 200 && status < 300),
    headers: {
      get: (name: string) => response.headers?.[name] ?? null,
    },
    json: () => Promise.resolve(response.body),
  });
}

describe('FraiseQLClient', () => {
  describe('constructor', () => {
    it('accepts a string URL', () => {
      const client = new FraiseQLClient('http://localhost:4000/graphql');
      expect(client).toBeInstanceOf(FraiseQLClient);
    });

    it('accepts a config object', () => {
      const client = new FraiseQLClient({
        url: 'http://localhost:4000/graphql',
        timeoutMs: 5000,
      });
      expect(client).toBeInstanceOf(FraiseQLClient);
    });
  });

  describe('query', () => {
    it('returns data on success', async () => {
      const fetchMock = makeFetch({
        body: { data: { users: [{ id: '1', name: 'Alice' }] } },
      });
      const client = new FraiseQLClient({
        url: 'http://localhost:4000/graphql',
        fetch: fetchMock as unknown as typeof fetch,
      });

      const result = await client.query('{ users { id name } }');
      expect(result).toEqual({ users: [{ id: '1', name: 'Alice' }] });
    });

    it('sends variables in request body', async () => {
      const fetchMock = makeFetch({
        body: { data: { user: { id: '42' } } },
      });
      const client = new FraiseQLClient({
        url: 'http://localhost:4000/graphql',
        fetch: fetchMock as unknown as typeof fetch,
      });

      await client.query('query($id: ID!) { user(id: $id) { id } }', { id: '42' });

      const callBody = JSON.parse(
        (fetchMock.mock.calls[0] as [string, { body: string }])[1].body
      );
      expect(callBody.variables).toEqual({ id: '42' });
    });

    it('includes Authorization header when auth is a string', async () => {
      const fetchMock = makeFetch({ body: { data: {} } });
      const client = new FraiseQLClient({
        url: 'http://localhost:4000/graphql',
        authorization: 'Bearer token123',
        fetch: fetchMock as unknown as typeof fetch,
      });

      await client.query('{ __typename }');

      const headers = (fetchMock.mock.calls[0] as [string, { headers: Record<string, string> }])[1].headers;
      expect(headers['Authorization']).toBe('Bearer token123');
    });

    it('includes Authorization header from async function', async () => {
      const fetchMock = makeFetch({ body: { data: {} } });
      const client = new FraiseQLClient({
        url: 'http://localhost:4000/graphql',
        authorization: async () => 'Bearer async-token',
        fetch: fetchMock as unknown as typeof fetch,
      });

      await client.query('{ __typename }');

      const headers = (fetchMock.mock.calls[0] as [string, { headers: Record<string, string> }])[1].headers;
      expect(headers['Authorization']).toBe('Bearer async-token');
    });

    it('throws GraphQLError when errors array is present and non-empty', async () => {
      const fetchMock = makeFetch({
        body: {
          data: null,
          errors: [{ message: 'Field not found', path: ['user'] }],
        },
      });
      const client = new FraiseQLClient({
        url: 'http://localhost:4000/graphql',
        fetch: fetchMock as unknown as typeof fetch,
      });

      await expect(client.query('{ user { id } }')).rejects.toBeInstanceOf(GraphQLError);
    });

    it('null errors array is NOT an error (regression)', async () => {
      const fetchMock = makeFetch({
        body: { data: { ping: true }, errors: null },
      });
      const client = new FraiseQLClient({
        url: 'http://localhost:4000/graphql',
        fetch: fetchMock as unknown as typeof fetch,
      });

      const result = await client.query('{ ping }');
      expect(result).toEqual({ ping: true });
    });

    it('absent errors field is NOT an error', async () => {
      const fetchMock = makeFetch({
        body: { data: { health: 'ok' } },
      });
      const client = new FraiseQLClient({
        url: 'http://localhost:4000/graphql',
        fetch: fetchMock as unknown as typeof fetch,
      });

      const result = await client.query('{ health }');
      expect(result).toEqual({ health: 'ok' });
    });

    it('throws AuthenticationError on 401', async () => {
      const fetchMock = makeFetch({ status: 401, ok: false, statusText: 'Unauthorized' });
      const client = new FraiseQLClient({
        url: 'http://localhost:4000/graphql',
        fetch: fetchMock as unknown as typeof fetch,
      });

      const err = await client.query('{ user }').catch((e: unknown) => e);
      expect(err).toBeInstanceOf(AuthenticationError);
      expect((err as AuthenticationError).statusCode).toBe(401);
    });

    it('throws AuthenticationError on 403', async () => {
      const fetchMock = makeFetch({ status: 403, ok: false, statusText: 'Forbidden' });
      const client = new FraiseQLClient({
        url: 'http://localhost:4000/graphql',
        fetch: fetchMock as unknown as typeof fetch,
      });

      await expect(client.query('{ secret }')).rejects.toBeInstanceOf(AuthenticationError);
    });

    it('throws RateLimitError on 429', async () => {
      const fetchMock = makeFetch({
        status: 429,
        ok: false,
        headers: { 'Retry-After': '60' },
      });
      const client = new FraiseQLClient({
        url: 'http://localhost:4000/graphql',
        fetch: fetchMock as unknown as typeof fetch,
      });

      const err = await client.query('{ data }').catch((e: unknown) => e);
      expect(err).toBeInstanceOf(RateLimitError);
      expect((err as RateLimitError).retryAfterMs).toBe(60000);
    });

    it('throws NetworkError on non-ok HTTP status', async () => {
      const fetchMock = makeFetch({ status: 500, ok: false, statusText: 'Internal Server Error' });
      const client = new FraiseQLClient({
        url: 'http://localhost:4000/graphql',
        fetch: fetchMock as unknown as typeof fetch,
      });

      await expect(client.query('{ data }')).rejects.toBeInstanceOf(NetworkError);
    });

    it('throws NetworkError when fetch throws', async () => {
      const fetchMock = vi.fn().mockRejectedValue(new TypeError('Failed to fetch'));
      const client = new FraiseQLClient({
        url: 'http://localhost:4000/graphql',
        fetch: fetchMock as unknown as typeof fetch,
      });

      await expect(client.query('{ data }')).rejects.toBeInstanceOf(NetworkError);
    });

    it('throws HttpStatusError on a client-side 4xx', async () => {
      const fetchMock = makeFetch({ status: 404, ok: false, statusText: 'Not Found' });
      const client = new FraiseQLClient({
        url: 'http://localhost:4000/graphql',
        fetch: fetchMock as unknown as typeof fetch,
      });

      const err = await client.query('{ data }').catch((e: unknown) => e);
      expect(err).toBeInstanceOf(HttpStatusError);
      expect((err as HttpStatusError).status).toBe(404);
    });

    it('throws TimeoutError on 408', async () => {
      const fetchMock = makeFetch({ status: 408, ok: false, statusText: 'Request Timeout' });
      const client = new FraiseQLClient({
        url: 'http://localhost:4000/graphql',
        fetch: fetchMock as unknown as typeof fetch,
      });

      await expect(client.query('{ data }')).rejects.toBeInstanceOf(TimeoutError);
    });
  });

  describe('retry classification (ADR-0015 §3: 4xx is permanent)', () => {
    it('does not retry a 4xx', async () => {
      const fetchMock = makeFetch({ status: 400, ok: false, statusText: 'Bad Request' });
      const client = new FraiseQLClient({
        url: 'http://localhost:4000/graphql',
        retry: { maxAttempts: 3, baseDelayMs: 0, jitter: false },
        fetch: fetchMock as unknown as typeof fetch,
      });

      await expect(client.query('{ data }')).rejects.toBeInstanceOf(HttpStatusError);
      expect(fetchMock).toHaveBeenCalledTimes(1);
    });

    it('retries a 5xx', async () => {
      const fetchMock = makeFetch({ status: 503, ok: false, statusText: 'Service Unavailable' });
      const client = new FraiseQLClient({
        url: 'http://localhost:4000/graphql',
        retry: { maxAttempts: 3, baseDelayMs: 0, jitter: false },
        fetch: fetchMock as unknown as typeof fetch,
      });

      await expect(client.query('{ data }')).rejects.toBeInstanceOf(NetworkError);
      expect(fetchMock).toHaveBeenCalledTimes(3);
    });
  });

  describe('mutate', () => {
    it('sends mutation as query field', async () => {
      const fetchMock = makeFetch({
        body: { data: { createUser: { id: '1' } } },
      });
      const client = new FraiseQLClient({
        url: 'http://localhost:4000/graphql',
        fetch: fetchMock as unknown as typeof fetch,
      });

      const result = await client.mutate(
        'mutation CreateUser($name: String!) { createUser(name: $name) { id } }',
        { name: 'Bob' }
      );
      expect(result).toEqual({ createUser: { id: '1' } });
    });
  });

  describe('idempotency (#1060)', () => {
    function headersOf(fetchMock: ReturnType<typeof vi.fn>, call: number): Record<string, string> {
      return (fetchMock.mock.calls[call]?.[1] as { headers: Record<string, string> }).headers;
    }

    it('sends an explicit idempotencyKey as the Idempotency-Key header', async () => {
      const fetchMock = makeFetch({ body: { data: { createOrder: { id: '1' } } } });
      const client = new FraiseQLClient({
        url: 'http://localhost:4000/graphql',
        fetch: fetchMock as unknown as typeof fetch,
      });

      await client.mutate('mutation { createOrder { id } }', undefined, undefined, {
        idempotencyKey: 'order-4711',
      });

      expect(headersOf(fetchMock, 0)['Idempotency-Key']).toBe('order-4711');
    });

    it('reuses one generated key across every retry attempt of a mutation', async () => {
      // The whole point: the server dedups by key, so the retries of ONE logical
      // call must not each look like a new request.
      const fetchMock = makeFetch({ status: 503, ok: false, statusText: 'Service Unavailable' });
      const client = new FraiseQLClient({
        url: 'http://localhost:4000/graphql',
        retry: { maxAttempts: 3, baseDelayMs: 0, jitter: false },
        fetch: fetchMock as unknown as typeof fetch,
      });

      await client.mutate('mutation { createOrder { id } }').catch(() => undefined);

      expect(fetchMock).toHaveBeenCalledTimes(3);
      const keys = [0, 1, 2].map((i) => headersOf(fetchMock, i)['Idempotency-Key']);
      expect(keys[0]).toBeTruthy();
      expect(new Set(keys).size).toBe(1);
    });

    it('generates two different keys for two separate mutate() calls', async () => {
      const fetchMock = makeFetch({ body: { data: { createOrder: { id: '1' } } } });
      const client = new FraiseQLClient({
        url: 'http://localhost:4000/graphql',
        retry: { maxAttempts: 3, baseDelayMs: 0, jitter: false },
        fetch: fetchMock as unknown as typeof fetch,
      });

      await client.mutate('mutation { createOrder { id } }');
      await client.mutate('mutation { createOrder { id } }');

      expect(headersOf(fetchMock, 0)['Idempotency-Key']).not.toBe(
        headersOf(fetchMock, 1)['Idempotency-Key']
      );
    });

    it('does not generate a key when retry is not enabled', async () => {
      const fetchMock = makeFetch({ body: { data: { createOrder: { id: '1' } } } });
      const client = new FraiseQLClient({
        url: 'http://localhost:4000/graphql',
        fetch: fetchMock as unknown as typeof fetch,
      });

      await client.mutate('mutation { createOrder { id } }');

      expect(headersOf(fetchMock, 0)['Idempotency-Key']).toBeUndefined();
    });

    it('does not generate a key for a query, which is already safe to repeat', async () => {
      const fetchMock = makeFetch({ body: { data: { users: [] } } });
      const client = new FraiseQLClient({
        url: 'http://localhost:4000/graphql',
        retry: { maxAttempts: 3, baseDelayMs: 0, jitter: false },
        fetch: fetchMock as unknown as typeof fetch,
      });

      await client.query('{ users { id } }');

      expect(headersOf(fetchMock, 0)['Idempotency-Key']).toBeUndefined();
    });

    it('merges per-call headers over the client-level ones', async () => {
      const fetchMock = makeFetch({ body: { data: { users: [] } } });
      const client = new FraiseQLClient({
        url: 'http://localhost:4000/graphql',
        headers: { 'X-Trace': 'client', 'X-Kept': 'yes' },
        fetch: fetchMock as unknown as typeof fetch,
      });

      await client.query('{ users { id } }', undefined, undefined, {
        headers: { 'X-Trace': 'per-call' },
      });

      const headers = headersOf(fetchMock, 0);
      expect(headers['X-Trace']).toBe('per-call');
      expect(headers['X-Kept']).toBe('yes');
    });
  });

  describe('timeout', () => {
    it('throws TimeoutError when AbortError is raised', async () => {
      const fetchMock = vi.fn().mockRejectedValue(
        Object.assign(new Error('The operation was aborted'), { name: 'AbortError' })
      );
      const client = new FraiseQLClient({
        url: 'http://localhost:4000/graphql',
        timeoutMs: 1,
        fetch: fetchMock as unknown as typeof fetch,
      });

      await expect(client.query('{ slow }')).rejects.toBeInstanceOf(TimeoutError);
    });

    it('bounds the response body read, not only the header round-trip', async () => {
      // Headers arrive at once; the body never completes unless the request is
      // aborted — the shape of a hung upstream or a half-closed socket. If the
      // deadline only covers `fetch`, nothing ever aborts and this never settles.
      const fetchMock = vi.fn((_url: string, init: { signal: AbortSignal }) =>
        Promise.resolve({
          status: 200,
          statusText: 'OK',
          ok: true,
          headers: { get: () => null },
          json: () =>
            new Promise((_resolve, reject) => {
              init.signal.addEventListener('abort', () => {
                reject(
                  Object.assign(new Error('The operation was aborted'), {
                    name: 'AbortError',
                  })
                );
              });
            }),
        })
      );

      const client = new FraiseQLClient({
        url: 'http://localhost:4000/graphql',
        timeoutMs: 50,
        fetch: fetchMock as unknown as typeof fetch,
      });

      // Race against a sentinel so a missing deadline fails an assertion rather
      // than hanging until the runner's own timeout.
      const outcome = await Promise.race([
        client.query('{ slow }').then(
          () => 'RESOLVED' as unknown,
          (error: unknown) => error
        ),
        new Promise<unknown>((resolve) =>
          setTimeout(() => resolve('NO_DEADLINE'), 1000)
        ),
      ]);

      expect(outcome).toBeInstanceOf(TimeoutError);
    });
  });
});
