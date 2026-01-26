import { describe, it, expect } from '@jest/globals';
import {
  AuthzPolicyBuilder,
  AuthzPolicyType,
  AuthzPolicy,
} from '../src/security';

describe('Attribute-Based Access Control', () => {
  it('should create ABAC policy definition', () => {
    const config = new AuthzPolicyBuilder('secretClearance')
      .type(AuthzPolicyType.ABAC)
      .description('Requires top secret clearance')
      .attributes('clearance_level >= 3', 'background_check == true')
      .build();

    expect(config.name).toBe('secretClearance');
    expect(config.type).toBe(AuthzPolicyType.ABAC);
    expect(config.attributes).toHaveLength(2);
  });

  it('should create ABAC with variadic attributes', () => {
    const config = new AuthzPolicyBuilder('financialData')
      .attributes(
        'clearance_level >= 2',
        'department == "finance"',
        'mfa_enabled == true'
      )
      .build();

    expect(config.attributes).toHaveLength(3);
    expect(config.attributes).toContain('clearance_level >= 2');
  });

  it('should create ABAC with array attributes', () => {
    const config = new AuthzPolicyBuilder('regionalData')
      .attributesArray(['region == "US"', 'gdpr_compliant == true'])
      .build();

    expect(config.attributes).toHaveLength(2);
  });

  it('should support clearance level pattern', () => {
    const config = new AuthzPolicyBuilder('classifiedDocument')
      .type(AuthzPolicyType.ABAC)
      .description('Access based on clearance level')
      .attributes('clearance_level >= 2')
      .build();

    expect(config.type).toBe(AuthzPolicyType.ABAC);
    expect(config.attributes).toHaveLength(1);
  });

  it('should support department pattern', () => {
    const config = new AuthzPolicyBuilder('departmentData')
      .type(AuthzPolicyType.ABAC)
      .attributes('department == "HR"')
      .description('HR department access only')
      .build();

    expect(config.name).toBe('departmentData');
  });

  it('should support time-based pattern', () => {
    const config = new AuthzPolicyBuilder('timeRestrictedData')
      .type(AuthzPolicyType.ABAC)
      .attributes(
        'current_time > "09:00"',
        'current_time < "17:00"',
        'day_of_week != "Sunday"'
      )
      .description('Business hours access')
      .build();

    expect(config.attributes).toHaveLength(3);
  });

  it('should support geographic pattern', () => {
    const config = new AuthzPolicyBuilder('geographicRestriction')
      .type(AuthzPolicyType.ABAC)
      .attributes('region in ["US", "CA", "MX"]')
      .description('North American access only')
      .build();

    expect(config.attributes).toHaveLength(1);
  });

  it('should support GDPR compliance pattern', () => {
    const config = new AuthzPolicyBuilder('personalData')
      .type(AuthzPolicyType.ABAC)
      .attributes(
        'gdpr_compliant == true',
        'data_residency == "EU"',
        'consent_given == true'
      )
      .description('GDPR-compliant access')
      .build();

    expect(config.attributes).toHaveLength(3);
  });

  it('should support project-based pattern', () => {
    const config = new AuthzPolicyBuilder('projectData')
      .type(AuthzPolicyType.ABAC)
      .attributes('user_project == resource_project')
      .description('Users can only access their own projects')
      .build();

    expect(config.attributes).toHaveLength(1);
  });

  it('should support data classification pattern', () => {
    const config = new AuthzPolicyBuilder('dataClassification')
      .type(AuthzPolicyType.ABAC)
      .attributes(
        'user_classification >= resource_classification',
        'has_need_to_know == true'
      )
      .description('Classification-based access control')
      .build();

    expect(config.attributes).toHaveLength(2);
  });

  it('should support ABAC caching', () => {
    const config = new AuthzPolicyBuilder('cachedAbac')
      .type(AuthzPolicyType.ABAC)
      .attributes('attribute1 == "value"')
      .cacheable(true)
      .cacheDurationSeconds(3600)
      .build();

    expect(config.cacheable).toBe(true);
    expect(config.cacheDurationSeconds).toBe(3600);
  });

  it('should support ABAC without cache', () => {
    const config = new AuthzPolicyBuilder('sensitiveAbac')
      .type(AuthzPolicyType.ABAC)
      .attributes('sensitive_attribute == true')
      .cacheable(false)
      .build();

    expect(config.cacheable).toBe(false);
  });

  it('should support ABAC audit logging', () => {
    const config = new AuthzPolicyBuilder('auditedAbac')
      .type(AuthzPolicyType.ABAC)
      .attributes('access_control == true')
      .auditLogging(true)
      .build();

    expect(config.auditLogging).toBe(true);
  });

  it('should support ABAC error message', () => {
    const config = new AuthzPolicyBuilder('restrictedAbac')
      .type(AuthzPolicyType.ABAC)
      .attributes('clearance_level >= 3')
      .errorMessage('Your clearance level is insufficient for this resource')
      .build();

    expect(config.errorMessage).toBe(
      'Your clearance level is insufficient for this resource'
    );
  });

  it('should support operation-specific ABAC', () => {
    const config = new AuthzPolicyBuilder('deleteRestricted')
      .type(AuthzPolicyType.ABAC)
      .attributes('role == "admin"')
      .operations('delete,create')
      .build();

    expect(config.operations).toBe('delete,create');
  });

  it('should support recursive ABAC', () => {
    const config = new AuthzPolicyBuilder('recursiveAbac')
      .type(AuthzPolicyType.ABAC)
      .attributes('hierarchy_level >= 2')
      .recursive(true)
      .build();

    expect(config.recursive).toBe(true);
  });

  it('should support fluent chaining', () => {
    const config = new AuthzPolicyBuilder('complexAbac')
      .type(AuthzPolicyType.ABAC)
      .description('Complex ABAC policy')
      .attributes('clearance >= 2', 'department == "IT"', 'mfa == true')
      .cacheable(true)
      .cacheDurationSeconds(1800)
      .recursive(false)
      .operations('read,update')
      .auditLogging(true)
      .errorMessage('Access denied')
      .build();

    expect(config.name).toBe('complexAbac');
    expect(config.type).toBe(AuthzPolicyType.ABAC);
    expect(config.attributes).toHaveLength(3);
    expect(config.cacheable).toBe(true);
    expect(config.auditLogging).toBe(true);
  });

  it('should support attributes with rule', () => {
    const config = new AuthzPolicyBuilder('hybridAbac')
      .type(AuthzPolicyType.ABAC)
      .rule("hasAttribute($context, 'clearance_level', 3)")
      .attributes('clearance_level >= 3')
      .build();

    expect(config.rule).toBe("hasAttribute($context, 'clearance_level', 3)");
  });

  it('should support decorator syntax', () => {
    @AuthzPolicy({
      name: 'abacExample',
      type: AuthzPolicyType.ABAC,
      attributes: ['clearance >= 2', 'department == "Finance"'],
    })
    class AbacExample {
      data: string;
    }

    expect(AbacExample).toBeDefined();
  });
});
