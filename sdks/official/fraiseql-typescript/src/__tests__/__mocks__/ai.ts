// Test stub for the 'ai' peer dependency
export const tool = jest.fn(
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
