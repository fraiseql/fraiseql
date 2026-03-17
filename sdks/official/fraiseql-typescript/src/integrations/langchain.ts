/**
 * LangChain.js integration for FraiseQL.
 *
 * @example
 * ```typescript
 * import { FraiseQLTool } from 'fraiseql/integrations/langchain';
 * import { z } from 'zod';
 *
 * const tool = new FraiseQLTool(client, {
 *   name: 'getUser',
 *   description: 'Fetch a user by ID',
 *   query: `query GetUser($id: ID!) { user(id: $id) { id name email } }`,
 *   schema: z.object({ id: z.string() }),
 * });
 * ```
 */

import { StructuredTool } from '@langchain/core/tools';
import type { z } from 'zod';
import type { FraiseQLClient } from '../client';

export class FraiseQLTool extends StructuredTool {
  schema: z.ZodObject<z.ZodRawShape>;
  name: string;
  description: string;
  private readonly client: FraiseQLClient;
  private readonly gqlQuery: string;

  constructor(
    client: FraiseQLClient,
    options: {
      name: string;
      description: string;
      query: string;
      schema: z.ZodObject<z.ZodRawShape>;
    }
  ) {
    super();
    this.client = client;
    this.name = options.name;
    this.description = options.description;
    this.gqlQuery = options.query;
    this.schema = options.schema;
  }

  protected async _call(input: Record<string, unknown>): Promise<string> {
    const result = await this.client.query(this.gqlQuery, input);
    return JSON.stringify(result);
  }
}
