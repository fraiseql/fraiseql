import { vi } from 'vitest';
import { FraiseQLClient } from '../client';
import {
  GraphQLError,
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
  });
});
