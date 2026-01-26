import { describe, it, expect } from '@jest/globals';
import {
  AuthorizeBuilder,
  AuthorizeConfig,
  Authorize,
} from '../src/security';

describe('Authorization Rules', () => {
  it('should create authorization rule builder', () => {
    const config = new AuthorizeBuilder()
      .rule("isOwner($context.userId, $field.ownerId)")
      .description("Ensures users can only access their own notes")
      .build();

    expect(config.rule).toBe("isOwner($context.userId, $field.ownerId)");
    expect(config.description).toBe("Ensures users can only access their own notes");
  });

  it('should create authorization with policy reference', () => {
    const config = new AuthorizeBuilder()
      .policy("piiAccess")
      .description("References the piiAccess policy")
      .build();

    expect(config.policy).toBe("piiAccess");
    expect(config.cacheable).toBe(true);
  });

  it('should create authorization with error message', () => {
    const config = new AuthorizeBuilder()
      .rule("hasRole($context, 'admin')")
      .errorMessage("Only administrators can access this resource")
      .build();

    expect(config.errorMessage).toBe("Only administrators can access this resource");
  });

  it('should create recursive authorization', () => {
    const config = new AuthorizeBuilder()
      .rule("canAccessNested($context)")
      .recursive(true)
      .description("Recursively applies to nested types")
      .build();

    expect(config.recursive).toBe(true);
  });

  it('should create operation-specific authorization', () => {
    const config = new AuthorizeBuilder()
      .rule("isAdmin($context)")
      .operations("create,delete")
      .description("Only applies to create and delete operations")
      .build();

    expect(config.operations).toBe("create,delete");
  });

  it('should create authorization with caching', () => {
    const config = new AuthorizeBuilder()
      .rule("checkAuthorization($context)")
      .cacheable(true)
      .cacheDurationSeconds(3600)
      .build();

    expect(config.cacheable).toBe(true);
    expect(config.cacheDurationSeconds).toBe(3600);
  });

  it('should create authorization without caching', () => {
    const config = new AuthorizeBuilder()
      .rule("checkSensitiveAuthorization($context)")
      .cacheable(false)
      .build();

    expect(config.cacheable).toBe(false);
  });

  it('should create multiple authorization rules', () => {
    const config1 = new AuthorizeBuilder()
      .rule("isOwner($context.userId, $field.ownerId)")
      .description("Ownership check")
      .build();

    const config2 = new AuthorizeBuilder()
      .rule("hasScope($context, 'read:notes')")
      .description("Scope check")
      .build();

    expect(config1.rule).not.toBe(config2.rule);
  });

  it('should support fluent chaining', () => {
    const config = new AuthorizeBuilder()
      .rule("isOwner($context.userId, $field.ownerId)")
      .description("Ownership authorization")
      .errorMessage("You can only access your own notes")
      .recursive(false)
      .operations("read,update")
      .cacheable(true)
      .cacheDurationSeconds(600)
      .build();

    expect(config.rule).toBe("isOwner($context.userId, $field.ownerId)");
    expect(config.description).toBe("Ownership authorization");
    expect(config.errorMessage).toBe("You can only access your own notes");
    expect(config.recursive).toBe(false);
    expect(config.operations).toBe("read,update");
    expect(config.cacheable).toBe(true);
    expect(config.cacheDurationSeconds).toBe(600);
  });

  it('should support decorator syntax', () => {
    @Authorize({
      rule: "isOwner($context.userId, $field.ownerId)",
      description: "Ownership check",
    })
    class ProtectedNote {
      id: number;
      content: string;
      ownerId: string;
    }

    expect(ProtectedNote).toBeDefined();
  });

  it('should support full decorator configuration', () => {
    @Authorize({
      rule: "isOwner($context.userId, $field.ownerId)",
      description: "Ownership check",
      errorMessage: "Access denied",
      recursive: true,
      operations: "read",
      cacheable: false,
      cacheDurationSeconds: 0,
    })
    class FullyConfiguredNote {
      id: number;
    }

    expect(FullyConfiguredNote).toBeDefined();
  });
});
