require 'spec_helper'
require_relative '../lib/fraiseql/security'

describe FraiseQL::Security do
  describe 'AuthzPolicyBuilder' do
    it 'creates RBAC policy' do
      config = FraiseQL::Security::AuthzPolicyBuilder.create('adminOnly')
        .type(FraiseQL::Security::AuthzPolicyType::RBAC)
        .rule("hasRole($context, 'admin')")
        .description('Access restricted to administrators')
        .audit_logging(true)
        .build

      expect(config.name).to eq('adminOnly')
      expect(config.type).to eq(FraiseQL::Security::AuthzPolicyType::RBAC)
      expect(config.rule).to eq("hasRole($context, 'admin')")
      expect(config.audit_logging).to be true
    end

    it 'creates ABAC policy' do
      config = FraiseQL::Security::AuthzPolicyBuilder.create('secretClearance')
        .type(FraiseQL::Security::AuthzPolicyType::ABAC)
        .description('Requires top secret clearance')
        .attributes('clearance_level >= 3', 'background_check == true')
        .build

      expect(config.name).to eq('secretClearance')
      expect(config.type).to eq(FraiseQL::Security::AuthzPolicyType::ABAC)
      expect(config.attributes).to have(2).items
    end

    it 'creates CUSTOM policy' do
      config = FraiseQL::Security::AuthzPolicyBuilder.create('customRule')
        .type(FraiseQL::Security::AuthzPolicyType::CUSTOM)
        .rule("isOwner($context.userId, $resource.ownerId)")
        .description('Custom ownership rule')
        .build

      expect(config.type).to eq(FraiseQL::Security::AuthzPolicyType::CUSTOM)
    end

    it 'creates HYBRID policy' do
      config = FraiseQL::Security::AuthzPolicyBuilder.create('auditAccess')
        .type(FraiseQL::Security::AuthzPolicyType::HYBRID)
        .description('Role and attribute-based access')
        .rule("hasRole($context, 'auditor')")
        .attributes('audit_enabled == true')
        .build

      expect(config.type).to eq(FraiseQL::Security::AuthzPolicyType::HYBRID)
      expect(config.rule).to eq("hasRole($context, 'auditor')")
    end

    it 'creates multiple policies' do
      policy1 = FraiseQL::Security::AuthzPolicyBuilder.create('policy1')
        .type(FraiseQL::Security::AuthzPolicyType::RBAC)
        .build

      policy2 = FraiseQL::Security::AuthzPolicyBuilder.create('policy2')
        .type(FraiseQL::Security::AuthzPolicyType::ABAC)
        .build

      policy3 = FraiseQL::Security::AuthzPolicyBuilder.create('policy3')
        .type(FraiseQL::Security::AuthzPolicyType::CUSTOM)
        .build

      expect(policy1.name).to eq('policy1')
      expect(policy2.name).to eq('policy2')
      expect(policy3.name).to eq('policy3')
    end

    it 'creates PII access policy' do
      config = FraiseQL::Security::AuthzPolicyBuilder.create('piiAccess')
        .type(FraiseQL::Security::AuthzPolicyType::RBAC)
        .description('Access to Personally Identifiable Information')
        .rule("hasRole($context, 'data_manager') OR hasScope($context, 'read:pii')")
        .build

      expect(config.name).to eq('piiAccess')
    end

    it 'creates admin only policy' do
      config = FraiseQL::Security::AuthzPolicyBuilder.create('adminOnly')
        .type(FraiseQL::Security::AuthzPolicyType::RBAC)
        .description('Admin-only access')
        .rule("hasRole($context, 'admin')")
        .audit_logging(true)
        .build

      expect(config.audit_logging).to be true
    end

    it 'creates recursive policy' do
      config = FraiseQL::Security::AuthzPolicyBuilder.create('recursiveProtection')
        .type(FraiseQL::Security::AuthzPolicyType::CUSTOM)
        .rule("canAccessNested($context)")
        .recursive(true)
        .description('Recursively applies to nested types')
        .build

      expect(config.recursive).to be true
    end

    it 'creates operation-specific policy' do
      config = FraiseQL::Security::AuthzPolicyBuilder.create('readOnly')
        .type(FraiseQL::Security::AuthzPolicyType::CUSTOM)
        .rule("hasRole($context, 'viewer')")
        .operations('read')
        .description('Policy applies only to read operations')
        .build

      expect(config.operations).to eq('read')
    end

    it 'creates cached policy' do
      config = FraiseQL::Security::AuthzPolicyBuilder.create('cachedAccess')
        .type(FraiseQL::Security::AuthzPolicyType::CUSTOM)
        .rule("hasRole($context, 'viewer')")
        .cacheable(true)
        .cache_duration_seconds(3600)
        .description('Access control with result caching')
        .build

      expect(config.cacheable).to be true
      expect(config.cache_duration_seconds).to eq(3600)
    end

    it 'creates audited policy' do
      config = FraiseQL::Security::AuthzPolicyBuilder.create('auditedAccess')
        .type(FraiseQL::Security::AuthzPolicyType::RBAC)
        .rule("hasRole($context, 'auditor')")
        .audit_logging(true)
        .description('Access with comprehensive audit logging')
        .build

      expect(config.audit_logging).to be true
    end

    it 'creates policy with error message' do
      config = FraiseQL::Security::AuthzPolicyBuilder.create('restrictedAccess')
        .type(FraiseQL::Security::AuthzPolicyType::RBAC)
        .rule("hasRole($context, 'executive')")
        .error_message('Only executive level users can access this resource')
        .build

      expect(config.error_message).to eq('Only executive level users can access this resource')
    end

    it 'supports fluent chaining' do
      config = FraiseQL::Security::AuthzPolicyBuilder.create('complexPolicy')
        .type(FraiseQL::Security::AuthzPolicyType::HYBRID)
        .description('Complex hybrid policy')
        .rule("hasRole($context, 'admin')")
        .attributes('security_clearance >= 3')
        .cacheable(true)
        .cache_duration_seconds(1800)
        .recursive(false)
        .operations('create,update,delete')
        .audit_logging(true)
        .error_message('Insufficient privileges')
        .build

      expect(config.name).to eq('complexPolicy')
      expect(config.type).to eq(FraiseQL::Security::AuthzPolicyType::HYBRID)
      expect(config.cacheable).to be true
      expect(config.audit_logging).to be true
    end

    it 'creates policy composition' do
      public_policy = FraiseQL::Security::AuthzPolicyBuilder.create('publicAccess')
        .type(FraiseQL::Security::AuthzPolicyType::RBAC)
        .rule('true')  # Everyone has access
        .build

      pii_policy = FraiseQL::Security::AuthzPolicyBuilder.create('piiAccess')
        .type(FraiseQL::Security::AuthzPolicyType::RBAC)
        .rule("hasRole($context, 'data_manager')")
        .build

      admin_policy = FraiseQL::Security::AuthzPolicyBuilder.create('adminAccess')
        .type(FraiseQL::Security::AuthzPolicyType::RBAC)
        .rule("hasRole($context, 'admin')")
        .build

      expect(public_policy.name).to eq('publicAccess')
      expect(pii_policy.name).to eq('piiAccess')
      expect(admin_policy.name).to eq('adminAccess')
    end

    it 'creates financial data policy' do
      config = FraiseQL::Security::AuthzPolicyBuilder.create('financialData')
        .type(FraiseQL::Security::AuthzPolicyType::ABAC)
        .description('Access to financial records')
        .attributes('clearance_level >= 2', 'department == "finance"')
        .build

      expect(config.name).to eq('financialData')
      expect(config.attributes).to have(2).items
    end

    it 'creates security clearance policy' do
      config = FraiseQL::Security::AuthzPolicyBuilder.create('secretClearance')
        .type(FraiseQL::Security::AuthzPolicyType::ABAC)
        .attributes('clearance_level >= 3', 'background_check == true')
        .description('Requires top secret clearance')
        .build

      expect(config.attributes).to have(2).items
    end

    it 'supports include basic syntax' do
      class AdminPolicy
        include FraiseQL::Security::AuthzPolicy

        authz_policy name: 'adminOnly',
                     rule: "hasRole($context, 'admin')"
      end

      expect(AdminPolicy.policy_config[:name]).to eq('adminOnly')
    end

    it 'supports include with all parameters' do
      class ComplexPolicy
        include FraiseQL::Security::AuthzPolicy

        authz_policy name: 'complexPolicy',
                     type: FraiseQL::Security::AuthzPolicyType::HYBRID,
                     description: 'Complex policy',
                     rule: "hasRole($context, 'admin')",
                     attributes: ['clearance >= 3'],
                     error_message: 'Access denied',
                     recursive: true,
                     operations: 'delete,create',
                     audit_logging: true,
                     cacheable: true,
                     cache_duration_seconds: 1800
      end

      config = ComplexPolicy.policy_config
      expect(config[:name]).to eq('complexPolicy')
      expect(config[:type]).to eq(FraiseQL::Security::AuthzPolicyType::HYBRID)
    end

    it 'supports all policy types' do
      rbac = FraiseQL::Security::AuthzPolicyBuilder.create('rbac')
        .type(FraiseQL::Security::AuthzPolicyType::RBAC)
        .build

      abac = FraiseQL::Security::AuthzPolicyBuilder.create('abac')
        .type(FraiseQL::Security::AuthzPolicyType::ABAC)
        .build

      custom = FraiseQL::Security::AuthzPolicyBuilder.create('custom')
        .type(FraiseQL::Security::AuthzPolicyType::CUSTOM)
        .build

      hybrid = FraiseQL::Security::AuthzPolicyBuilder.create('hybrid')
        .type(FraiseQL::Security::AuthzPolicyType::HYBRID)
        .build

      expect(rbac.type).to eq(FraiseQL::Security::AuthzPolicyType::RBAC)
      expect(abac.type).to eq(FraiseQL::Security::AuthzPolicyType::ABAC)
      expect(custom.type).to eq(FraiseQL::Security::AuthzPolicyType::CUSTOM)
      expect(hybrid.type).to eq(FraiseQL::Security::AuthzPolicyType::HYBRID)
    end

    it 'creates default configuration' do
      config = FraiseQL::Security::AuthzPolicyBuilder.create('default').build

      expect(config.name).to eq('default')
      expect(config.type).to eq(FraiseQL::Security::AuthzPolicyType::CUSTOM)
      expect(config.cacheable).to be true
      expect(config.cache_duration_seconds).to eq(300)
    end
  end
end
