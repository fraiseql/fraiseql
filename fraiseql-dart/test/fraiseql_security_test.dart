import 'package:test/test.dart';
import 'package:fraiseql_dart/fraiseql_security.dart';

void main() {
  group('AuthorizationTests', () {
    test('should create simple authorization rule', () {
      final config = AuthorizeBuilder()
          .rule('isOwner(\$context.userId, \$field.ownerId)')
          .description('Ownership check')
          .build();

      expect(config.rule, 'isOwner(\$context.userId, \$field.ownerId)');
      expect(config.description, 'Ownership check');
    });

    test('should create authorization with policy', () {
      final config = AuthorizeBuilder()
          .policy('ownerOnly')
          .description('References named policy')
          .build();

      expect(config.policy, 'ownerOnly');
    });

    test('should support fluent chaining', () {
      final config = AuthorizeBuilder()
          .rule('hasPermission(\$context)')
          .description('Complex rule')
          .errorMessage('Access denied')
          .recursive(true)
          .operations('read')
          .build();

      expect(config.rule, 'hasPermission(\$context)');
      expect(config.recursive, true);
      expect(config.operations, 'read');
    });

    test('should set caching configuration', () {
      final config = AuthorizeBuilder()
          .rule('checkAccess(\$context)')
          .cacheable(true)
          .cacheDurationSeconds(600)
          .build();

      expect(config.cacheable, true);
      expect(config.cacheDurationSeconds, 600);
    });

    test('should set error message', () {
      final config = AuthorizeBuilder()
          .rule('adminOnly(\$context)')
          .errorMessage('Only administrators can access this')
          .build();

      expect(config.errorMessage, 'Only administrators can access this');
    });

    test('should set recursive application', () {
      final config = AuthorizeBuilder()
          .rule('checkNested(\$context)')
          .recursive(true)
          .description('Applied to nested types')
          .build();

      expect(config.recursive, true);
    });

    test('should set operation specific rule', () {
      final config = AuthorizeBuilder()
          .rule('canDelete(\$context)')
          .operations('delete')
          .description('Only applies to delete operations')
          .build();

      expect(config.operations, 'delete');
    });

    test('should convert to map', () {
      final config = AuthorizeBuilder()
          .rule('testRule')
          .description('Test')
          .build();

      final map = config.toMap();

      expect(map['rule'], 'testRule');
      expect(map['description'], 'Test');
    });

    test('should create multiple configurations', () {
      final config1 = AuthorizeBuilder().rule('rule1').build();
      final config2 = AuthorizeBuilder().rule('rule2').build();

      expect(config1.rule, isNot(equals(config2.rule)));
    });

    test('should return default cache settings', () {
      final config = AuthorizeBuilder().rule('test').build();

      expect(config.cacheable, true);
      expect(config.cacheDurationSeconds, 300);
    });

    test('should set all options', () {
      final config = AuthorizeBuilder()
          .rule('complex')
          .policy('policy')
          .description('Complex authorization')
          .errorMessage('Error')
          .recursive(true)
          .operations('create,read,update,delete')
          .cacheable(false)
          .cacheDurationSeconds(1000)
          .build();

      expect(config.rule, 'complex');
      expect(config.cacheable, false);
      expect(config.cacheDurationSeconds, 1000);
    });
  });

  group('RoleBasedAccessControlTests', () {
    test('should create single role requirement', () {
      final config =
          RoleRequiredBuilder().roles(['admin']).build();

      expect(config.roles.length, 1);
      expect(config.roles[0], 'admin');
    });

    test('should create multiple role requirements', () {
      final config = RoleRequiredBuilder()
          .roles(['manager', 'director'])
          .build();

      expect(config.roles.length, 2);
      expect(config.roles, contains('manager'));
      expect(config.roles, contains('director'));
    });

    test('should use any role matching strategy', () {
      final config = RoleRequiredBuilder()
          .roles(['viewer', 'editor'])
          .strategy(RoleMatchStrategy.any)
          .description('At least one role')
          .build();

      expect(config.strategy, RoleMatchStrategy.any);
    });

    test('should use all role matching strategy', () {
      final config = RoleRequiredBuilder()
          .roles(['admin', 'auditor'])
          .strategy(RoleMatchStrategy.all)
          .description('All roles required')
          .build();

      expect(config.strategy, RoleMatchStrategy.all);
    });

    test('should use exactly role matching strategy', () {
      final config = RoleRequiredBuilder()
          .roles(['exact_role'])
          .strategy(RoleMatchStrategy.exactly)
          .description('Exactly these roles')
          .build();

      expect(config.strategy, RoleMatchStrategy.exactly);
    });

    test('should support role hierarchy', () {
      final config = RoleRequiredBuilder()
          .roles(['admin'])
          .hierarchy(true)
          .description('With hierarchy')
          .build();

      expect(config.hierarchy, true);
    });

    test('should support role inheritance', () {
      final config = RoleRequiredBuilder()
          .roles(['editor'])
          .inherit(true)
          .description('Inherits from parent')
          .build();

      expect(config.inherit, true);
    });

    test('should set operation specific roles', () {
      final config = RoleRequiredBuilder()
          .roles(['editor'])
          .operations('create,update')
          .description('Only for edit operations')
          .build();

      expect(config.operations, 'create,update');
    });

    test('should set custom error message', () {
      final config = RoleRequiredBuilder()
          .roles(['admin'])
          .errorMessage('Administrator access required')
          .build();

      expect(config.errorMessage, 'Administrator access required');
    });

    test('should configure caching', () {
      final config = RoleRequiredBuilder()
          .roles(['viewer'])
          .cacheable(true)
          .cacheDurationSeconds(1800)
          .build();

      expect(config.cacheable, true);
      expect(config.cacheDurationSeconds, 1800);
    });

    test('should create admin pattern', () {
      final config = RoleRequiredBuilder()
          .roles(['admin'])
          .strategy(RoleMatchStrategy.any)
          .description('Admin access')
          .build();

      expect(config.roles.length, 1);
      expect(config.roles[0], 'admin');
    });

    test('should create manager director pattern', () {
      final config = RoleRequiredBuilder()
          .roles(['manager', 'director'])
          .strategy(RoleMatchStrategy.any)
          .description('Managers and directors')
          .build();

      expect(config.roles.length, 2);
      expect(config.strategy, RoleMatchStrategy.any);
    });

    test('should create data scientist pattern', () {
      final config = RoleRequiredBuilder()
          .roles(['data_scientist', 'analyst'])
          .strategy(RoleMatchStrategy.any)
          .description('Data professionals')
          .build();

      expect(config.roles.length, 2);
    });

    test('should convert to map', () {
      final config = RoleRequiredBuilder()
          .roles(['admin', 'editor'])
          .strategy(RoleMatchStrategy.any)
          .build();

      final map = config.toMap();

      expect(map['strategy'], 'any');
    });

    test('should set description', () {
      final config = RoleRequiredBuilder()
          .roles(['viewer'])
          .description('Read-only access')
          .build();

      expect(config.description, 'Read-only access');
    });

    test('should return default values', () {
      final config = RoleRequiredBuilder().roles(['user']).build();

      expect(config.hierarchy, false);
      expect(config.inherit, false);
      expect(config.cacheable, true);
      expect(config.cacheDurationSeconds, 300);
    });
  });

  group('AttributeBasedAccessControlTests', () {
    test('should create ABAC policy', () {
      final config = AuthzPolicyBuilder('accessControl')
          .type(AuthzPolicyType.abac)
          .attributes(['clearance_level >= 2'])
          .description('Basic clearance')
          .build();

      expect(config.name, 'accessControl');
      expect(config.type, AuthzPolicyType.abac);
    });

    test('should handle multiple attributes', () {
      final config = AuthzPolicyBuilder('secretAccess')
          .type(AuthzPolicyType.abac)
          .attributes(['clearance_level >= 3', 'background_check == true'])
          .build();

      expect(config.attributes.length, 2);
    });

    test('should create clearance level policy', () {
      final config = AuthzPolicyBuilder('topSecret')
          .type(AuthzPolicyType.abac)
          .attributes(['clearance_level >= 3'])
          .description('Top secret clearance required')
          .build();

      expect(config.attributes.length, 1);
    });

    test('should create department policy', () {
      final config = AuthzPolicyBuilder('financeDept')
          .type(AuthzPolicyType.abac)
          .attributes(['department == \"finance\"'])
          .description('Finance department only')
          .build();

      expect(config.name, 'financeDept');
    });

    test('should create time based policy', () {
      final config = AuthzPolicyBuilder('businessHours')
          .type(AuthzPolicyType.abac)
          .attributes(['now >= 9:00 AM', 'now <= 5:00 PM'])
          .description('During business hours')
          .build();

      expect(config.attributes.length, 2);
    });

    test('should create geographic policy', () {
      final config = AuthzPolicyBuilder('usOnly')
          .type(AuthzPolicyType.abac)
          .attributes(['country == \"US\"'])
          .description('United States only')
          .build();

      expect(config.attributes.length, 1);
    });

    test('should create GDPR policy', () {
      final config = AuthzPolicyBuilder('gdprCompliance')
          .type(AuthzPolicyType.abac)
          .attributes(['gdpr_compliant == true', 'data_residency == \"EU\"'])
          .description('GDPR compliance required')
          .build();

      expect(config.attributes.length, 2);
    });

    test('should create data classification policy', () {
      final config = AuthzPolicyBuilder('classifiedData')
          .type(AuthzPolicyType.abac)
          .attributes(['classification >= 2'])
          .description('For classified documents')
          .build();

      expect(config.attributes.length, 1);
    });

    test('should support caching in ABAC policy', () {
      final config = AuthzPolicyBuilder('cachedAccess')
          .type(AuthzPolicyType.abac)
          .attributes(['role == \"viewer\"'])
          .cacheable(true)
          .cacheDurationSeconds(600)
          .build();

      expect(config.cacheable, true);
      expect(config.cacheDurationSeconds, 600);
    });

    test('should support audit logging in ABAC policy', () {
      final config = AuthzPolicyBuilder('auditedAccess')
          .type(AuthzPolicyType.abac)
          .attributes(['audit_enabled == true'])
          .auditLogging(true)
          .build();

      expect(config.auditLogging, true);
    });

    test('should support recursive application in ABAC policy', () {
      final config = AuthzPolicyBuilder('recursiveAccess')
          .type(AuthzPolicyType.abac)
          .attributes(['permission >= 1'])
          .recursive(true)
          .description('Applies to nested types')
          .build();

      expect(config.recursive, true);
    });

    test('should set operation specific attribute policy', () {
      final config = AuthzPolicyBuilder('readOnly')
          .type(AuthzPolicyType.abac)
          .attributes(['can_read == true'])
          .operations('read')
          .build();

      expect(config.operations, 'read');
    });

    test('should create complex ABAC policy', () {
      final config = AuthzPolicyBuilder('complex')
          .type(AuthzPolicyType.abac)
          .attributes(['level >= 2', 'verified == true', 'active == true'])
          .description('Complex attribute rules')
          .auditLogging(true)
          .cacheable(true)
          .build();

      expect(config.attributes.length, 3);
      expect(config.auditLogging, true);
    });

    test('should set error message in ABAC policy', () {
      final config = AuthzPolicyBuilder('restricted')
          .type(AuthzPolicyType.abac)
          .attributes(['clearance >= 2'])
          .errorMessage('Insufficient clearance level')
          .build();

      expect(config.errorMessage, 'Insufficient clearance level');
    });

    test('should support to map conversion', () {
      final config = AuthzPolicyBuilder('test')
          .type(AuthzPolicyType.abac)
          .attributes(['test >= 1'])
          .build();

      final map = config.toMap();

      expect(map['type'], 'abac');
    });
  });

  group('AuthzPolicyTests', () {
    test('should create RBAC policy', () {
      final config = AuthzPolicyBuilder('adminOnly')
          .type(AuthzPolicyType.rbac)
          .rule('hasRole(\$context, \'admin\')')
          .description('Access restricted to administrators')
          .auditLogging(true)
          .build();

      expect(config.name, 'adminOnly');
      expect(config.type, AuthzPolicyType.rbac);
      expect(config.rule, 'hasRole(\$context, \'admin\')');
      expect(config.auditLogging, true);
    });

    test('should create ABAC policy full', () {
      final config = AuthzPolicyBuilder('secretClearance')
          .type(AuthzPolicyType.abac)
          .description('Requires top secret clearance')
          .attributes(['clearance_level >= 3', 'background_check == true'])
          .build();

      expect(config.name, 'secretClearance');
      expect(config.type, AuthzPolicyType.abac);
      expect(config.attributes.length, 2);
    });

    test('should create custom policy', () {
      final config = AuthzPolicyBuilder('customRule')
          .type(AuthzPolicyType.custom)
          .rule('isOwner(\$context.userId, \$resource.ownerId)')
          .description('Custom ownership rule')
          .build();

      expect(config.type, AuthzPolicyType.custom);
    });

    test('should create hybrid policy', () {
      final config = AuthzPolicyBuilder('auditAccess')
          .type(AuthzPolicyType.hybrid)
          .description('Role and attribute-based access')
          .rule('hasRole(\$context, \'auditor\')')
          .attributes(['audit_enabled == true'])
          .build();

      expect(config.type, AuthzPolicyType.hybrid);
      expect(config.rule, 'hasRole(\$context, \'auditor\')');
    });

    test('should create multiple policies', () {
      final p1 = AuthzPolicyBuilder('policy1')
          .type(AuthzPolicyType.rbac)
          .build();
      final p2 = AuthzPolicyBuilder('policy2')
          .type(AuthzPolicyType.abac)
          .build();
      final p3 = AuthzPolicyBuilder('policy3')
          .type(AuthzPolicyType.custom)
          .build();

      expect(p1.name, 'policy1');
      expect(p2.name, 'policy2');
      expect(p3.name, 'policy3');
    });

    test('should create PII access policy', () {
      final config = AuthzPolicyBuilder('piiAccess')
          .type(AuthzPolicyType.rbac)
          .rule('hasRole(\$context, \'data_manager\')')
          .build();

      expect(config.name, 'piiAccess');
    });

    test('should create admin only policy', () {
      final config = AuthzPolicyBuilder('adminOnly')
          .type(AuthzPolicyType.rbac)
          .auditLogging(true)
          .build();

      expect(config.auditLogging, true);
    });

    test('should create recursive policy', () {
      final config = AuthzPolicyBuilder('recursiveProtection')
          .type(AuthzPolicyType.custom)
          .recursive(true)
          .build();

      expect(config.recursive, true);
    });

    test('should create operation specific policy', () {
      final config = AuthzPolicyBuilder('readOnly')
          .type(AuthzPolicyType.custom)
          .operations('read')
          .build();

      expect(config.operations, 'read');
    });

    test('should create cached policy', () {
      final config = AuthzPolicyBuilder('cachedAccess')
          .type(AuthzPolicyType.custom)
          .cacheable(true)
          .cacheDurationSeconds(3600)
          .build();

      expect(config.cacheable, true);
      expect(config.cacheDurationSeconds, 3600);
    });

    test('should create audited policy', () {
      final config = AuthzPolicyBuilder('auditedAccess')
          .type(AuthzPolicyType.rbac)
          .auditLogging(true)
          .build();

      expect(config.auditLogging, true);
    });

    test('should create policy with error message', () {
      final config = AuthzPolicyBuilder('restrictedAccess')
          .type(AuthzPolicyType.rbac)
          .errorMessage('Only executive level users can access this resource')
          .build();

      expect(config.errorMessage,
          'Only executive level users can access this resource');
    });

    test('should support fluent chaining', () {
      final config = AuthzPolicyBuilder('complexPolicy')
          .type(AuthzPolicyType.hybrid)
          .rule('hasRole(\$context, \'admin\')')
          .attributes(['security_clearance >= 3'])
          .cacheable(true)
          .cacheDurationSeconds(1800)
          .recursive(false)
          .operations('create,update,delete')
          .auditLogging(true)
          .errorMessage('Insufficient privileges')
          .build();

      expect(config.name, 'complexPolicy');
      expect(config.type, AuthzPolicyType.hybrid);
      expect(config.cacheable, true);
      expect(config.auditLogging, true);
    });

    test('should create policy composition', () {
      final p1 = AuthzPolicyBuilder('publicAccess')
          .type(AuthzPolicyType.rbac)
          .rule('true')
          .build();
      final p2 = AuthzPolicyBuilder('piiAccess')
          .type(AuthzPolicyType.rbac)
          .build();
      final p3 = AuthzPolicyBuilder('adminAccess')
          .type(AuthzPolicyType.rbac)
          .build();

      expect(p1.name, 'publicAccess');
      expect(p2.name, 'piiAccess');
      expect(p3.name, 'adminAccess');
    });

    test('should create financial data policy', () {
      final config = AuthzPolicyBuilder('financialData')
          .type(AuthzPolicyType.abac)
          .attributes(['clearance_level >= 2', 'department == \"finance\"'])
          .build();

      expect(config.name, 'financialData');
      expect(config.attributes.length, 2);
    });

    test('should create security clearance policy', () {
      final config = AuthzPolicyBuilder('secretClearance')
          .type(AuthzPolicyType.abac)
          .attributes(['clearance_level >= 3', 'background_check == true'])
          .build();

      expect(config.attributes.length, 2);
    });

    test('should create default configuration', () {
      final config = AuthzPolicyBuilder('default').build();

      expect(config.name, 'default');
      expect(config.type, AuthzPolicyType.custom);
      expect(config.cacheable, true);
      expect(config.cacheDurationSeconds, 300);
    });

    test('should convert to map', () {
      final config = AuthzPolicyBuilder('test')
          .type(AuthzPolicyType.rbac)
          .rule('test_rule')
          .build();

      final map = config.toMap();

      expect(map['name'], 'test');
      expect(map['type'], 'rbac');
    });
  });
}
