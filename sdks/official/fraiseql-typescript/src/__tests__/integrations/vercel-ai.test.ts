// Mock the 'ai' module before importing the integration
jest.mock('ai', () => ({
  tool: jest.fn(
    (config: {
      description: string;
      parameters: unknown;
      execute: (p: unknown) => unknown;
    }) => ({
      _isTool: true,
      description: config.description,
      parameters: config.parameters,
      execute: config.execute,
    })
  ),
}));

import { fraiseqlTool } from '../../integrations/vercel-ai';
import { FraiseQLClient } from '../../client';

function makeMockClient(data: Record<string, unknown>): FraiseQLClient {
  const fetchMock = jest.fn().mockResolvedValue({
    status: 200,
    ok: true,
    statusText: 'OK',
    headers: { get: () => null },
    json: () => Promise.resolve({ data }),
  });
  return new FraiseQLClient({
    url: 'http://localhost:4000/graphql',
    fetch: fetchMock as unknown as typeof fetch,
  });
}

// Minimal zod-like schema mock for testing without zod installed
const mockSchema = {
  _type: {} as Record<string, unknown>,
  parse: (v: unknown) => v,
} as unknown as import('zod').z.ZodObject<import('zod').z.ZodRawShape>;

describe('fraiseqlTool (Vercel AI)', () => {
  it('creates a tool with description and parameters', () => {
    const client = makeMockClient({});
    const t = fraiseqlTool(client, {
      name: 'getUser',
      description: 'Fetch a user',
      query: 'query($id: ID!) { user(id: $id) { id } }',
      parameters: mockSchema,
    });

    expect(t).toHaveProperty('description', 'Fetch a user');
    expect(t).toHaveProperty('execute');
  });

  it('execute calls client.query with params', async () => {
    const userData = { user: { id: '1', name: 'Alice' } };
    const client = makeMockClient(userData);
    const querySpy = jest.spyOn(client, 'query');

    const t = fraiseqlTool(client, {
      name: 'getUser',
      description: 'Fetch a user',
      query: 'query($id: ID!) { user(id: $id) { id name } }',
      parameters: mockSchema,
    });

    const result = await (
      t as { execute: (p: Record<string, unknown>) => Promise<unknown> }
    ).execute({ id: '1' });

    expect(querySpy).toHaveBeenCalledWith(
      'query($id: ID!) { user(id: $id) { id name } }',
      { id: '1' }
    );
    expect(result).toEqual(userData);
  });

  it('applies transform function when provided', async () => {
    const client = makeMockClient({ users: [{ id: '1' }, { id: '2' }] });

    const t = fraiseqlTool(client, {
      name: 'listUsers',
      description: 'List users',
      query: '{ users { id } }',
      parameters: mockSchema,
      transform: (data) =>
        (data['users'] as { id: string }[]).map((u) => u.id),
    });

    const result = await (
      t as { execute: (p: Record<string, unknown>) => Promise<unknown> }
    ).execute({});
    expect(result).toEqual(['1', '2']);
  });
});
