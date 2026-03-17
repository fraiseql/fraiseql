// Test stub for the 'ai' peer dependency
import { vi } from 'vitest';

export const tool = vi.fn(
  (config: {
    description: string;
    parameters: unknown;
    execute: (params: unknown) => unknown;
  }) => ({
    _isTool: true,
    description: config.description,
    parameters: config.parameters,
    execute: config.execute,
  })
);
