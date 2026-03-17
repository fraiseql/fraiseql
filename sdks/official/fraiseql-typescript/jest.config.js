module.exports = {
  preset: "ts-jest",
  testEnvironment: "node",
  roots: ["<rootDir>/tests", "<rootDir>/src/__tests__"],
  testMatch: ["**/__tests__/**/*.test.ts", "**/?(*.)+(spec|test).ts"],
  testPathIgnorePatterns: ["/node_modules/", "/__mocks__/"],
  moduleNameMapper: {
    "^ai$": "<rootDir>/src/__tests__/__mocks__/ai.ts",
  },
  moduleFileExtensions: ["ts", "js", "json"],
  collectCoverageFrom: [
    "src/**/*.ts",
    "!src/**/*.d.ts",
    "!src/**/index.ts"
  ],
  coveragePathIgnorePatterns: ["/node_modules/"],
  globals: {
    "ts-jest": {
      diagnostics: false,
      tsconfig: {
        esModuleInterop: true,
        strict: false,
        strictNullChecks: false,
        noImplicitAny: false,
        noUnusedLocals: false,
      },
    },
  },
};
