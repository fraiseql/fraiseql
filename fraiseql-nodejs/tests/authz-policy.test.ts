import { describe, it, expect } from '@jest/globals';
import {
  AuthzPolicyBuilder,
  AuthzPolicyType,
  AuthzPolicy,
} from '../src/security';

describe('Authorization Policies', () => {
  it('should create RBAC policy', () => {
    const config = new AuthzPolicyBuilder('adminOnly')
      .type(AuthzPolicyType.RBAC)
      .rule("hasRole($context, 'admin')")
      .description('Access restricted to administrators')
      .auditLogging(true)
      .build();

    expect(config.name).toBe('adminOnly');
    expect(config.type).toBe(AuthzPolicyType.RBAC);
    expect(config.rule).toBe("hasRole($context, 'admin')");
    expect(config.auditLogging).toBe(true);
  });

  it('should create ABAC policy', () => {
    const config = new AuthzPolicyBuilder('secretClearance')
      .type(AuthzPolicyType.ABAC)
      .description('Requires top secret clearance')
      .attributes('clearance_level >= 3', 'background_check == true')
      .build();

    expect(config.name).toBe('secretClearance');
    expect(config.type).toBe(AuthzPolicyType.ABAC);
    expect(config.attributes).toHaveLength(2);
  });

  it('should create CUSTOM policy', () => {
    const config = new AuthzPolicyBuilder('customRule')
      .type(AuthzPolicyType.CUSTOM)
      .rule("isOwner($context.userId, $resource.ownerId)")
      .description('Custom ownership rule')
      .build();

    expect(config.type).toBe(AuthzPolicyType.CUSTOM);
  });

  it('should create HYBRID policy', () => {
    const config = new AuthzPolicyBuilder('auditAccess')
      .type(AuthzPolicyType.HYBRID)
      .description('Role and attribute-based access')
      .rule("hasRole($context, 'auditor')")
      .attributes('audit_enabled == true')
      .build();

    expect(config.type).toBe(AuthzPolicyType.HYBRID);
    expect(config.rule).toBe("hasRole($context, 'auditor')");
  });

  it('should create multiple policies', () => {
    const policy1 = new AuthzPolicyBuilder('policy1')
      .type(AuthzPolicyType.RBAC)
      .build();

    const policy2 = new AuthzPolicyBuilder('policy2')
      .type(AuthzPolicyType.ABAC)
      .build();

    const policy3 = new AuthzPolicyBuilder('policy3')
      .type(AuthzPolicyType.CUSTOM)
      .build();

    expect(policy1.name).toBe('policy1');
    expect(policy2.name).toBe('policy2');
    expect(policy3.name).toBe('policy3');
  });

  it('should create PII access policy', () => {
    const config = new AuthzPolicyBuilder('piiAccess')
      .type(AuthzPolicyType.RBAC)
      .description('Access to Personally Identifiable Information')
      .rule("hasRole($context, 'data_manager') OR hasScope($context, 'read:pii')")
      .build();

    expect(config.name).toBe('piiAccess');
  });

  it('should create admin only policy', () => {
    const config = new AuthzPolicyBuilder('adminOnly')
      .type(AuthzPolicyType.RBAC)
      .description('Admin-only access')
      .rule("hasRole($context, 'admin')")
      .auditLogging(true)
      .build();

    expect(config.auditLogging).toBe(true);
  });

  it('should create recursive policy', () => {
    const config = new AuthzPolicyBuilder('recursiveProtection')
      .type(AuthzPolicyType.CUSTOM)
      .rule("canAccessNested($context)")
      .recursive(true)
      .description('Recursively applies to nested types')
      .build();

    expect(config.recursive).toBe(true);
  });

  it('should create operation-specific policy', () => {
    const config = new AuthzPolicyBuilder('readOnly')
      .type(AuthzPolicyType.CUSTOM)
      .rule("hasRole($context, 'viewer')")
      .operations('read')
      .description('Policy applies only to read operations')
      .build();

    expect(config.operations).toBe('read');
  });

  it('should create cached policy', () => {
    const config = new AuthzPolicyBuilder('cachedAccess')
      .type(AuthzPolicyType.CUSTOM)
      .rule("hasRole($context, 'viewer')")
      .cacheable(true)
      .cacheDurationSeconds(3600)
      .description('Access control with result caching')
      .build();

    expect(config.cacheable).toBe(true);
    expect(config.cacheDurationSeconds).toBe(3600);
  });

  it('should create audited policy', () => {
    const config = new AuthzPolicyBuilder('auditedAccess')
      .type(AuthzPolicyType.RBAC)
      .rule("hasRole($context, 'auditor')")
      .auditLogging(true)
      .description('Access with comprehensive audit logging')
      .build();

    expect(config.auditLogging).toBe(true);
  });

  it('should create policy with error message', () => {
    const config = new AuthzPolicyBuilder('restrictedAccess')
      .type(AuthzPolicyType.RBAC)
      .rule("hasRole($context, 'executive')")
      .errorMessage('Only executive level users can access this resource')
      .build();

    expect(config.errorMessage).toBe(
      'Only executive level users can access this resource'
    );
  });

  it('should support fluent chaining', () => {
    const config = new AuthzPolicyBuilder('complexPolicy')
      .type(AuthzPolicyType.HYBRID)
      .description('Complex hybrid policy')
      .rule("hasRole($context, 'admin')")
      .attributes('security_clearance >= 3')
      .cacheable(true)
      .cacheDurationSeconds(1800)
      .recursive(false)
      .operations('create,update,delete')
      .auditLogging(true)
      .errorMessage('Insufficient privileges')
      .build();

    expect(config.name).toBe('complexPolicy');
    expect(config.type).toBe(AuthzPolicyType.HYBRID);
    expect(config.cacheable).toBe(true);
    expect(config.auditLogging).toBe(true);
  });

  it('should create policy composition', () => {
    const publicPolicy = new AuthzPolicyBuilder('publicAccess')
      .type(AuthzPolicyType.RBAC)
      .rule('true')  // Everyone has access
      .build();

    const piiPolicy = new AuthzPolicyBuilder('piiAccess')
      .type(AuthzPolicyType.RBAC)
      .rule("hasRole($context, 'data_manager')")
      .build();

    const adminPolicy = new AuthzPolicyBuilder('adminAccess')
      .type(AuthzPolicyType.RBAC)
      .rule("hasRole($context, 'admin')")
      .build();

    expect(publicPolicy.name).toBe('publicAccess');
    expect(piiPolicy.name).toBe('piiAccess');
    expect(adminPolicy.name).toBe('adminAccess');
  });

  it('should create financial data policy', () => {
    const config = new AuthzPolicyBuilder('financialData')
      .type(AuthzPolicyType.ABAC)
      .description('Access to financial records')
      .attributes('clearance_level >= 2', 'department == "finance"')
      .build();

    expect(config.name).toBe('financialData');
    expect(config.attributes).toHaveLength(2);
  });

  it('should create security clearance policy', () => {
    const config = new AuthzPolicyBuilder('secretClearance')
      .type(AuthzPolicyType.ABAC)
      .attributes('clearance_level >= 3', 'background_check == true')
      .description('Requires top secret clearance')
      .build();

    expect(config.attributes).toHaveLength(2);
  });

  it('should support decorator basic syntax', () => {
    @AuthzPolicy({
      name: 'adminOnly',
      rule: "hasRole($context, 'admin')",
    })
    class AdminPolicy {
      content: string;
    }

    expect(AdminPolicy).toBeDefined();
  });

  it('should support decorator with all parameters', () => {
    @AuthzPolicy({
      name: 'complexPolicy',
      type: AuthzPolicyType.HYBRID,
      description: 'Complex policy',
      rule: "hasRole($context, 'admin')",
      attributes: ['clearance >= 3'],
      errorMessage: 'Access denied',
      recursive: true,
      operations: 'delete,create',
      auditLogging: true,
      cacheable: true,
      cacheDurationSeconds: 1800,
    })
    class ComplexPolicy {
      data: string;
    }

    expect(ComplexPolicy).toBeDefined();
  });

  it('should support all policy types', () => {
    const rbac = new AuthzPolicyBuilder('rbac')
      .type(AuthzPolicyType.RBAC)
      .build();

    const abac = new AuthzPolicyBuilder('abac')
      .type(AuthzPolicyType.ABAC)
      .build();

    const custom = new AuthzPolicyBuilder('custom')
      .type(AuthzPolicyType.CUSTOM)
      .build();

    const hybrid = new AuthzPolicyBuilder('hybrid')
      .type(AuthzPolicyType.HYBRID)
      .build();

    expect(rbac.type).toBe(AuthzPolicyType.RBAC);
    expect(abac.type).toBe(AuthzPolicyType.ABAC);
    expect(custom.type).toBe(AuthzPolicyType.CUSTOM);
    expect(hybrid.type).toBe(AuthzPolicyType.HYBRID);
  });

  it('should create default configuration', () => {
    const config = new AuthzPolicyBuilder('default').build();

    expect(config.name).toBe('default');
    expect(config.type).toBe(AuthzPolicyType.CUSTOM);
    expect(config.cacheable).toBe(true);
    expect(config.cacheDurationSeconds).toBe(300);
  });
});
