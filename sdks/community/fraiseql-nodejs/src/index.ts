/**
 * FraiseQL Node.js - Minimal TypeScript/JavaScript GraphQL SDK for TOML-based workflow
 *
 * This SDK provides type definitions and schema export for use with fraiseql.toml:
 * - Type registration and introspection
 * - Minimal types.json export (types only)
 * - All operational config (queries, mutations, federation, security, observers) in TOML
 */

export { Schema } from './schema';

// Version info
export const VERSION = '2.0.0';
