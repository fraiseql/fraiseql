/**
 * FraiseQL Node.js - TypeScript/JavaScript GraphQL Schema Authoring
 *
 * Provides declarative, type-safe GraphQL schema definitions with:
 * - Advanced authorization and security
 * - Role-based access control (RBAC)
 * - Attribute-based access control (ABAC)
 * - Authorization policies
 * - 100% feature parity with Python, Java, Go, PHP, and TypeScript
 */

export {
  RoleMatchStrategy,
  AuthzPolicyType,
  AuthorizeConfig,
  RoleRequiredConfig,
  AuthzPolicyConfig,
  AuthorizeBuilder,
  RoleRequiredBuilder,
  AuthzPolicyBuilder,
  Authorize,
  RoleRequired,
  AuthzPolicy,
} from './security';

// Version info
export const VERSION = '1.0.0';
export const PARITY_LANGUAGES = ['Python', 'TypeScript', 'Java', 'Go', 'PHP', 'Node.js'];
