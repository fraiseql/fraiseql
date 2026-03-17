/**
 * Vercel AI SDK integration for FraiseQL.
 *
 * @example
 * ```typescript
 * import { fraiseqlTool } from 'fraiseql/integrations/vercel-ai';
 * import { z } from 'zod';
 *
 * const getUserTool = fraiseqlTool(client, {
 *   name: 'getUser',
 *   description: 'Fetch a user by ID',
 *   query: `query GetUser($id: ID!) { user(id: $id) { id name email } }`,
 *   parameters: z.object({ id: z.string() }),
 * });
 * ```
 */

import { tool } from 'ai';
import type { z } from 'zod';
import type { FraiseQLClient } from '../client';

export function fraiseqlTool<TParams extends z.ZodType>(
  client: FraiseQLClient,
  options: {
    name: string;
    description: string;
    query: string;
    parameters: TParams;
    transform?: (data: Record<string, unknown>) => unknown;
  }
): ReturnType<typeof tool> {
  return tool({
    description: options.description,
    parameters: options.parameters,
    execute: async (params) => {
      const data = await client.query(
        options.query,
        params as Record<string, unknown>
      );
      return options.transform
        ? options.transform(data as Record<string, unknown>)
        : data;
    },
  });
}
