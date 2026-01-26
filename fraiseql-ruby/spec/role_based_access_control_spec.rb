require 'spec_helper'
require_relative '../lib/fraiseql/security'

describe FraiseQL::Security do
  describe 'RoleRequiredBuilder' do
    it 'creates single role requirement' do
      config = FraiseQL::Security::RoleRequiredBuilder.create
        .roles('admin')
        .description('Admin role required')
        .build

      expect(config.roles).to have(1).item
      expect(config.roles).to include('admin')
    end

    it 'creates multiple role requirements' do
      config = FraiseQL::Security::RoleRequiredBuilder.create
        .roles('manager', 'director')
        .description('Manager or director required')
        .build

      expect(config.roles).to have(2).items
      expect(config.roles).to include('manager', 'director')
    end

    it 'creates roles from array' do
      config = FraiseQL::Security::RoleRequiredBuilder.create
        .roles_array(['viewer', 'editor', 'admin'])
        .description('Multiple roles via array')
        .build

      expect(config.roles).to have(3).items
    end

    it 'supports ANY matching strategy' do
      config = FraiseQL::Security::RoleRequiredBuilder.create
        .roles('manager', 'director')
        .strategy(FraiseQL::Security::RoleMatchStrategy::ANY)
        .description('User needs at least one role')
        .build

      expect(config.strategy).to eq(FraiseQL::Security::RoleMatchStrategy::ANY)
    end

    it 'supports ALL matching strategy' do
      config = FraiseQL::Security::RoleRequiredBuilder.create
        .roles('admin', 'auditor')
        .strategy(FraiseQL::Security::RoleMatchStrategy::ALL)
        .description('User needs all roles')
        .build

      expect(config.strategy).to eq(FraiseQL::Security::RoleMatchStrategy::ALL)
    end

    it 'supports EXACTLY matching strategy' do
      config = FraiseQL::Security::RoleRequiredBuilder.create
        .roles('admin')
        .strategy(FraiseQL::Security::RoleMatchStrategy::EXACTLY)
        .description('User must have exactly these roles')
        .build

      expect(config.strategy).to eq(FraiseQL::Security::RoleMatchStrategy::EXACTLY)
    end

    it 'supports role hierarchy' do
      config = FraiseQL::Security::RoleRequiredBuilder.create
        .roles('user')
        .hierarchy(true)
        .description('Role hierarchy enabled')
        .build

      expect(config.hierarchy).to be true
    end

    it 'supports role inheritance' do
      config = FraiseQL::Security::RoleRequiredBuilder.create
        .roles('editor')
        .inherit(true)
        .description('Inherit role requirements')
        .build

      expect(config.inherit).to be true
    end

    it 'supports operation-specific rules' do
      config = FraiseQL::Security::RoleRequiredBuilder.create
        .roles('admin')
        .operations('delete,create')
        .description('Admin for destructive operations')
        .build

      expect(config.operations).to eq('delete,create')
    end

    it 'supports caching' do
      config = FraiseQL::Security::RoleRequiredBuilder.create
        .roles('viewer')
        .cacheable(true)
        .cache_duration_seconds(1800)
        .build

      expect(config.cacheable).to be true
      expect(config.cache_duration_seconds).to eq(1800)
    end

    it 'supports custom error message' do
      config = FraiseQL::Security::RoleRequiredBuilder.create
        .roles('admin')
        .error_message('You must be an administrator to access this resource')
        .build

      expect(config.error_message).to eq('You must be an administrator to access this resource')
    end

    it 'supports fluent chaining' do
      config = FraiseQL::Security::RoleRequiredBuilder.create
        .roles('manager', 'director')
        .strategy(FraiseQL::Security::RoleMatchStrategy::ANY)
        .hierarchy(true)
        .description('Manager or director with hierarchy')
        .error_message('Insufficient role')
        .operations('read,update')
        .inherit(false)
        .cacheable(true)
        .cache_duration_seconds(900)
        .build

      expect(config.roles).to have(2).items
      expect(config.strategy).to eq(FraiseQL::Security::RoleMatchStrategy::ANY)
      expect(config.hierarchy).to be true
      expect(config.inherit).to be false
      expect(config.cache_duration_seconds).to eq(900)
    end

    it 'supports admin pattern' do
      config = FraiseQL::Security::RoleRequiredBuilder.create
        .roles('admin')
        .strategy(FraiseQL::Security::RoleMatchStrategy::EXACTLY)
        .hierarchy(true)
        .description('Full admin access with hierarchy')
        .build

      expect(config.roles).to have(1).item
      expect(config.hierarchy).to be true
    end

    it 'supports manager pattern' do
      config = FraiseQL::Security::RoleRequiredBuilder.create
        .roles('manager', 'director', 'executive')
        .strategy(FraiseQL::Security::RoleMatchStrategy::ANY)
        .description('Management tier access')
        .operations('read,create,update')
        .build

      expect(config.roles).to have(3).items
      expect(config.operations).to eq('read,create,update')
    end

    it 'supports data scientist pattern' do
      config = FraiseQL::Security::RoleRequiredBuilder.create
        .roles('data_scientist', 'analyst')
        .strategy(FraiseQL::Security::RoleMatchStrategy::ANY)
        .description('Data access for scientists and analysts')
        .operations('read')
        .build

      expect(config.roles).to have(2).items
    end

    it 'supports include syntax' do
      class AdminPanel
        include FraiseQL::Security::RoleRequired

        require_role roles: ['admin'],
                     description: 'Admin access required'
      end

      expect(AdminPanel.role_config[:roles]).to eq(['admin'])
    end

    it 'supports include with strategy' do
      class SalaryData
        include FraiseQL::Security::RoleRequired

        require_role roles: ['manager', 'director'],
                     strategy: FraiseQL::Security::RoleMatchStrategy::ANY,
                     description: 'Management access'
      end

      expect(SalaryData.role_config[:strategy]).to eq(FraiseQL::Security::RoleMatchStrategy::ANY)
    end

    it 'supports include with all parameters' do
      class ComplexRoleRequirement
        include FraiseQL::Security::RoleRequired

        require_role roles: ['admin', 'auditor'],
                     strategy: FraiseQL::Security::RoleMatchStrategy::ALL,
                     hierarchy: true,
                     description: 'Full admin with auditor',
                     error_message: 'Insufficient privileges',
                     operations: 'delete,create',
                     inherit: false,
                     cacheable: true,
                     cache_duration_seconds: 1200
      end

      config = ComplexRoleRequirement.role_config
      expect(config[:strategy]).to eq(FraiseQL::Security::RoleMatchStrategy::ALL)
      expect(config[:hierarchy]).to be true
    end

    it 'creates multiple roles with different strategies' do
      any = FraiseQL::Security::RoleRequiredBuilder.create
        .roles('editor', 'contributor')
        .strategy(FraiseQL::Security::RoleMatchStrategy::ANY)
        .build

      all = FraiseQL::Security::RoleRequiredBuilder.create
        .roles('editor', 'reviewer')
        .strategy(FraiseQL::Security::RoleMatchStrategy::ALL)
        .build

      exactly = FraiseQL::Security::RoleRequiredBuilder.create
        .roles('admin')
        .strategy(FraiseQL::Security::RoleMatchStrategy::EXACTLY)
        .build

      expect(any.strategy).to eq(FraiseQL::Security::RoleMatchStrategy::ANY)
      expect(all.strategy).to eq(FraiseQL::Security::RoleMatchStrategy::ALL)
      expect(exactly.strategy).to eq(FraiseQL::Security::RoleMatchStrategy::EXACTLY)
    end
  end

  describe 'RoleRequiredConfig' do
    it 'converts to hash' do
      config = FraiseQL::Security::RoleRequiredConfig.new(
        roles: ['admin'],
        strategy: FraiseQL::Security::RoleMatchStrategy::ANY,
        description: 'Admin access'
      )

      hash = config.to_h
      expect(hash[:roles]).to eq(['admin'])
      expect(hash[:description]).to eq('Admin access')
    end
  end
end
