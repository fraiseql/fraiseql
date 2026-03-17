/**
 * Minimal type stubs for optional peer dependencies.
 * These are replaced by the real packages when users install them.
 */

// Minimal zod stub for type-checking without the full package installed.
// When zod is installed as a dependency, its own types take precedence.
declare module 'zod' {
  export namespace z {
    type ZodRawShape = Record<string, ZodType>;
    interface ZodType {
      readonly _type: unknown;
    }
    interface ZodObject<T extends ZodRawShape> extends ZodType {
      readonly _shape: T;
    }
    type infer<T extends ZodType> = T['_type'];
  }
  export type { z };
}

declare module 'ai' {
  import type { z } from 'zod';

  export interface ToolConfig<TParams extends z.ZodType = z.ZodType> {
    description: string;
    parameters: TParams;
    execute: (params: z.infer<TParams>) => Promise<unknown> | unknown;
  }

  export interface Tool {
    description: string;
    parameters: unknown;
    execute: (params: unknown) => Promise<unknown> | unknown;
  }

  export function tool<TParams extends z.ZodType>(
    config: ToolConfig<TParams>
  ): Tool;
}

declare module '@langchain/core/tools' {
  import type { z } from 'zod';

  export abstract class StructuredTool {
    abstract schema: z.ZodObject<z.ZodRawShape>;
    abstract name: string;
    abstract description: string;
    protected abstract _call(input: Record<string, unknown>): Promise<string>;
    invoke(input: Record<string, unknown>): Promise<string>;
  }
}

declare module '@mastra/core/tools' {
  import type { z } from 'zod';

  export interface MastraToolConfig {
    id: string;
    description: string;
    inputSchema: z.ZodType;
    outputSchema?: z.ZodType;
    execute: (opts: { context: unknown }) => Promise<unknown> | unknown;
  }

  export interface MastraTool {
    id: string;
    description: string;
    execute: (opts: { context: unknown }) => Promise<unknown> | unknown;
  }

  export function createTool(config: MastraToolConfig): MastraTool;
}
