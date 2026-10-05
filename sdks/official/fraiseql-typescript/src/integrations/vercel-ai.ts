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

import { type Tool, tool } from "ai";
import type { z } from "zod";
import type { FraiseQLClient } from "../client";

export function fraiseqlTool<TParams extends z.ZodType>(
  client: FraiseQLClient,
  options: {
    name: string;
    description: string;
    query: string;
    parameters: TParams;
    transform?: (data: Record<string, unknown>) => unknown;
  }
): Tool<z.infer<TParams>, unknown> {
  return tool({
    description: options.description,
    // AI SDK 5+ reads the schema from `inputSchema`; under the old key `parameters` the
    // tool reached the model with no arguments at all.
    inputSchema: options.parameters,
    execute: async (params: z.infer<TParams>) => {
      const data = await client.query(options.query, params as Record<string, unknown>);
      return options.transform ? options.transform(data as Record<string, unknown>) : data;
    },
  });
}
