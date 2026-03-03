# Deprecated

This SDK has been deprecated and is no longer actively maintained.

**Deprecated since**: v2.0.0
**Last compatible schema version**: v1.x
**v2.0.0 compatibility**: Not supported. The v2 compiled schema format (`schema.compiled.json`) is not compatible with this SDK.

## Recommended Alternative

[FraiseQL TypeScript SDK](../../official/fraiseql-ts)

## Reason

The TypeScript SDK provides a superset of Node.js SDK functionality with full type safety and v2.0.0 support. It is the recommended SDK for all JavaScript/TypeScript runtimes including Node.js.

## Migration

1. Uninstall: `npm uninstall @fraiseql/node`
2. Install: `npm install @fraiseql/ts`
3. Update imports from `@fraiseql/node` → `@fraiseql/ts`.
4. The TypeScript SDK is a drop-in replacement — all existing query and mutation calls use the same API surface with added type inference.
5. Recompile your schema with `fraiseql-cli compile` to produce a v2 schema.
