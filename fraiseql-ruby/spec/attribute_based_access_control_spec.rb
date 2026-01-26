require 'spec_helper'
require_relative '../lib/fraiseql/security'

describe FraiseQL::Security do
  describe 'AuthzPolicyBuilder for ABAC' do
    it 'creates ABAC policy definition' do
      config = FraiseQL::Security::AuthzPolicyBuilder.create('secretClearance')
        .type(FraiseQL::Security::AuthzPolicyType::ABAC)
        .description('Requires top secret clearance')
        .attributes('clearance_level >= 3', 'background_check == true')
        .build

      expect(config.name).to eq('secretClearance')
      expect(config.type).to eq(FraiseQL::Security::AuthzPolicyType::ABAC)
      expect(config.attributes).to have(2).items
    end

    it 'creates ABAC with variadic attributes' do
      config = FraiseQL::Security::AuthzPolicyBuilder.create('financialData')
        .attributes(
          'clearance_level >= 2',
          'department == "finance"',
          'mfa_enabled == true'
        )
        .build

      expect(config.attributes).to have(3).items
      expect(config.attributes).to include('clearance_level >= 2')
    end

    it 'creates ABAC with array attributes' do
      config = FraiseQL::Security::AuthzPolicyBuilder.create('regionalData')
        .attributes_array(['region == "US"', 'gdpr_compliant == true'])
        .build

      expect(config.attributes).to have(2).items
    end

    it 'supports clearance level pattern' do
      config = FraiseQL::Security::AuthzPolicyBuilder.create('classifiedDocument')
        .type(FraiseQL::Security::AuthzPolicyType::ABAC)
        .description('Access based on clearance level')
        .attributes('clearance_level >= 2')
        .build

      expect(config.type).to eq(FraiseQL::Security::AuthzPolicyType::ABAC)
      expect(config.attributes).to have(1).item
    end

    it 'supports department pattern' do
      config = FraiseQL::Security::AuthzPolicyBuilder.create('departmentData')
        .type(FraiseQL::Security::AuthzPolicyType::ABAC)
        .attributes('department == "HR"')
        .description('HR department access only')
        .build

      expect(config.name).to eq('departmentData')
    end

    it 'supports time-based pattern' do
      config = FraiseQL::Security::AuthzPolicyBuilder.create('timeRestrictedData')
        .type(FraiseQL::Security::AuthzPolicyType::ABAC)
        .attributes(
          'current_time > "09:00"',
          'current_time < "17:00"',
          'day_of_week != "Sunday"'
        )
        .description('Business hours access')
        .build

      expect(config.attributes).to have(3).items
    end

    it 'supports geographic pattern' do
      config = FraiseQL::Security::AuthzPolicyBuilder.create('geographicRestriction')
        .type(FraiseQL::Security::AuthzPolicyType::ABAC)
        .attributes('region in ["US", "CA", "MX"]')
        .description('North American access only')
        .build

      expect(config.attributes).to have(1).item
    end

    it 'supports GDPR compliance pattern' do
      config = FraiseQL::Security::AuthzPolicyBuilder.create('personalData')
        .type(FraiseQL::Security::AuthzPolicyType::ABAC)
        .attributes(
          'gdpr_compliant == true',
          'data_residency == "EU"',
          'consent_given == true'
        )
        .description('GDPR-compliant access')
        .build

      expect(config.attributes).to have(3).items
    end

    it 'supports project-based pattern' do
      config = FraiseQL::Security::AuthzPolicyBuilder.create('projectData')
        .type(FraiseQL::Security::AuthzPolicyType::ABAC)
        .attributes('user_project == resource_project')
        .description('Users can only access their own projects')
        .build

      expect(config.attributes).to have(1).item
    end

    it 'supports data classification pattern' do
      config = FraiseQL::Security::AuthzPolicyBuilder.create('dataClassification')
        .type(FraiseQL::Security::AuthzPolicyType::ABAC)
        .attributes(
          'user_classification >= resource_classification',
          'has_need_to_know == true'
        )
        .description('Classification-based access control')
        .build

      expect(config.attributes).to have(2).items
    end

    it 'supports ABAC caching' do
      config = FraiseQL::Security::AuthzPolicyBuilder.create('cachedAbac')
        .type(FraiseQL::Security::AuthzPolicyType::ABAC)
        .attributes('attribute1 == "value"')
        .cacheable(true)
        .cache_duration_seconds(3600)
        .build

      expect(config.cacheable).to be true
      expect(config.cache_duration_seconds).to eq(3600)
    end

    it 'supports ABAC without cache' do
      config = FraiseQL::Security::AuthzPolicyBuilder.create('sensitiveAbac')
        .type(FraiseQL::Security::AuthzPolicyType::ABAC)
        .attributes('sensitive_attribute == true')
        .cacheable(false)
        .build

      expect(config.cacheable).to be false
    end

    it 'supports ABAC audit logging' do
      config = FraiseQL::Security::AuthzPolicyBuilder.create('auditedAbac')
        .type(FraiseQL::Security::AuthzPolicyType::ABAC)
        .attributes('access_control == true')
        .audit_logging(true)
        .build

      expect(config.audit_logging).to be true
    end

    it 'supports ABAC error message' do
      config = FraiseQL::Security::AuthzPolicyBuilder.create('restrictedAbac')
        .type(FraiseQL::Security::AuthzPolicyType::ABAC)
        .attributes('clearance_level >= 3')
        .error_message('Your clearance level is insufficient for this resource')
        .build

      expect(config.error_message).to eq('Your clearance level is insufficient for this resource')
    end

    it 'supports operation-specific ABAC' do
      config = FraiseQL::Security::AuthzPolicyBuilder.create('deleteRestricted')
        .type(FraiseQL::Security::AuthzPolicyType::ABAC)
        .attributes('role == "admin"')
        .operations('delete,create')
        .build

      expect(config.operations).to eq('delete,create')
    end

    it 'supports recursive ABAC' do
      config = FraiseQL::Security::AuthzPolicyBuilder.create('recursiveAbac')
        .type(FraiseQL::Security::AuthzPolicyType::ABAC)
        .attributes('hierarchy_level >= 2')
        .recursive(true)
        .build

      expect(config.recursive).to be true
    end

    it 'supports fluent chaining' do
      config = FraiseQL::Security::AuthzPolicyBuilder.create('complexAbac')
        .type(FraiseQL::Security::AuthzPolicyType::ABAC)
        .description('Complex ABAC policy')
        .attributes('clearance >= 2', 'department == "IT"', 'mfa == true')
        .cacheable(true)
        .cache_duration_seconds(1800)
        .recursive(false)
        .operations('read,update')
        .audit_logging(true)
        .error_message('Access denied')
        .build

      expect(config.name).to eq('complexAbac')
      expect(config.type).to eq(FraiseQL::Security::AuthzPolicyType::ABAC)
      expect(config.attributes).to have(3).items
      expect(config.cacheable).to be true
      expect(config.audit_logging).to be true
    end

    it 'supports attributes with rule' do
      config = FraiseQL::Security::AuthzPolicyBuilder.create('hybridAbac')
        .type(FraiseQL::Security::AuthzPolicyType::ABAC)
        .rule("hasAttribute($context, 'clearance_level', 3)")
        .attributes('clearance_level >= 3')
        .build

      expect(config.rule).to eq("hasAttribute($context, 'clearance_level', 3)")
    end

    it 'supports include syntax' do
      class AbacExample
        include FraiseQL::Security::AuthzPolicy

        authz_policy name: 'abacExample',
                     type: FraiseQL::Security::AuthzPolicyType::ABAC,
                     attributes: ['clearance >= 2', 'department == "Finance"']
      end

      expect(AbacExample.policy_config[:name]).to eq('abacExample')
    end
  end

  describe 'AuthzPolicyConfig' do
    it 'converts to hash' do
      config = FraiseQL::Security::AuthzPolicyConfig.new(
        name: 'testPolicy',
        type: FraiseQL::Security::AuthzPolicyType::ABAC,
        attributes: ['attr1', 'attr2']
      )

      hash = config.to_h
      expect(hash[:name]).to eq('testPolicy')
      expect(hash[:attributes]).to eq(['attr1', 'attr2'])
    end
  end
end
