/**
 * Mastra integration for FraiseQL.
 *
 * @example
 * ```typescript
 * import { fraiseqlMastraTool } from 'fraiseql/integrations/mastra';
 * import { z } from 'zod';
 *
 * const getUserTool = fraiseqlMastraTool(client, {
 *   id: 'getUser',
 *   description: 'Fetch a user by ID',
 *   query: `query GetUser($id: ID!) { user(id: $id) { id name email } }`,
 *   inputSchema: z.object({ id: z.string() }),
 * });
 * ```
 */

import { createTool } from '@mastra/core/tools';
import type { z } from 'zod';
import type { FraiseQLClient } from '../client';

export function fraiseqlMastraTool(
  client: FraiseQLClient,
  options: {
    id: string;
    description: string;
    query: string;
    inputSchema: z.ZodType;
    outputSchema?: z.ZodType;
  }
): ReturnType<typeof createTool> {
  return createTool({
    id: options.id,
    description: options.description,
    inputSchema: options.inputSchema,
    outputSchema: options.outputSchema,
    execute: async ({ context }) => {
      return client.query(
        options.query,
        context as Record<string, unknown>
      );
    },
  });
}
