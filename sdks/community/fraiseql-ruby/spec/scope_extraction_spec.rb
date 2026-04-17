# frozen_string_literal: true

require 'spec_helper'
require 'json'

# Field-Level RBAC for Ruby SDK
#
# Tests that field scopes are properly extracted from field configuration,
# stored in field registry, and exported to JSON for compiler consumption.
#
# RED Phase: 21 comprehensive test cases
# - 15 happy path tests for scope extraction and export
# - 6 validation tests for error handling
#
# Field format:
# - Single scope: { name: 'salary', type: 'Float', requires_scope: 'read:user.salary' }
# - Multiple scopes: { name: 'admin_notes', type: 'String', requires_scopes: ['admin', 'auditor'] }

RSpec.describe 'Ruby SDK Field Scope Extraction & Export' do
  before(:each) do
    FraiseQL::Schema.reset
  end

  # =========================================================================
  # HAPPY PATH: SINGLE SCOPE EXTRACTION (3 tests)
  # =========================================================================

  describe 'Single scope extraction' do
    it 'extracts single scope from field configuration' do
      # RED: This test fails because FieldInfo doesn't store scope
      FraiseQL::Schema.register_type('UserWithScope', [
                                       { name: 'id', type: 'Int' },
                                       { name: 'salary', type: 'Float', requires_scope: 'read:user.salary' }
                                     ])

      types = FraiseQL::SchemaRegistry.instance.all_types
      expect(types.length).to eq(1)

      user_type = types[0]
      expect(user_type[:fields].length).to eq(2)

      salary_field = user_type[:fields].find { |f| f[:name] == 'salary' }
      expect(salary_field).not_to be_nil
      expect(salary_field[:requires_scope]).to eq('read:user.salary')
    end

    it 'extracts multiple different scopes on different fields' do
      # RED: Tests extraction of different scopes on different fields
      FraiseQL::Schema.register_type('UserWithMultipleScopes', [
                                       { name: 'id', type: 'Int' },
                                       { name: 'email', type: 'String', requires_scope: 'read:user.email' },
                                       { name: 'phone', type: 'String', requires_scope: 'read:user.phone' },
                                       { name: 'ssn', type: 'String', requires_scope: 'read:user.ssn' }
                                     ])

      types = FraiseQL::SchemaRegistry.instance.all_types
      user_type = types[0]

      expect(user_type[:fields].find { |f| f[:name] == 'email' }[:requires_scope]).to eq('read:user.email')
      expect(user_type[:fields].find { |f| f[:name] == 'phone' }[:requires_scope]).to eq('read:user.phone')
      expect(user_type[:fields].find { |f| f[:name] == 'ssn' }[:requires_scope]).to eq('read:user.ssn')
    end

    it 'handles public fields without scope requirement' do
      # RED: Public fields should have nil/no scope
      FraiseQL::Schema.register_type('UserWithMixedFields', [
                                       { name: 'id', type: 'Int' },
                                       { name: 'name', type: 'String' },
                                       { name: 'email', type: 'String', requires_scope: 'read:user.email' }
                                     ])

      types = FraiseQL::SchemaRegistry.instance.all_types
      user_type = types[0]

      id_field = user_type[:fields].find { |f| f[:name] == 'id' }
      expect(id_field[:requires_scope]).to be_nil
    end
  end

  # =========================================================================
  # HAPPY PATH: MULTIPLE SCOPES ON SINGLE FIELD (3 tests)
  # =========================================================================

  describe 'Multiple scopes on single field' do
    it 'extracts multiple scopes on single field as array' do
      # RED: Field with requires_scopes array
      FraiseQL::Schema.register_type('AdminWithMultipleScopes', [
                                       { name: 'id', type: 'Int' },
                                       { name: 'admin_notes', type: 'String', requires_scopes: %w[admin:read auditor:read] }
                                     ])

      types = FraiseQL::SchemaRegistry.instance.all_types
      user_type = types[0]

      admin_field = user_type[:fields].find { |f| f[:name] == 'admin_notes' }
      expect(admin_field).not_to be_nil
      expect(admin_field[:requires_scopes]).not_to be_nil
      expect(admin_field[:requires_scopes]).to have_length(2)
      expect(admin_field[:requires_scopes]).to include('admin:read')
      expect(admin_field[:requires_scopes]).to include('auditor:read')
    end

    it 'mixes single-scope and multi-scope fields' do
      # RED: Type with both single-scope and multi-scope fields
      FraiseQL::Schema.register_type('MixedScopeTypes', [
                                       { name: 'basic_field', type: 'String', requires_scope: 'read:basic' },
                                       { name: 'advanced_field', type: 'String',
                                         requires_scopes: ['read:advanced', 'admin:read'] }
                                     ])

      types = FraiseQL::SchemaRegistry.instance.all_types
      type_def = types[0]

      expect(type_def[:fields].find { |f| f[:name] == 'basic_field' }[:requires_scope]).to eq('read:basic')
      expect(type_def[:fields].find { |f| f[:name] == 'advanced_field' }[:requires_scopes]).to have_length(2)
    end

    it 'preserves scope array order' do
      # RED: Scopes array order must be preserved
      FraiseQL::Schema.register_type('OrderedScopes', [
                                       { name: 'restricted', type: 'String', requires_scopes: %w[first:read second:read third:read] }
                                     ])

      types = FraiseQL::SchemaRegistry.instance.all_types
      type_def = types[0]

      scopes = type_def[:fields][0][:requires_scopes]
      expect(scopes).to have_length(3)
      expect(scopes[0]).to eq('first:read')
      expect(scopes[1]).to eq('second:read')
      expect(scopes[2]).to eq('third:read')
    end
  end

  # =========================================================================
  # HAPPY PATH: SCOPE PATTERNS (3 tests)
  # =========================================================================

  describe 'Scope patterns' do
    it 'supports resource-based scope pattern' do
      # RED: Resource pattern like read:User.email
      FraiseQL::Schema.register_type('ResourcePatternScopes', [
                                       { name: 'email', type: 'String', requires_scope: 'read:User.email' },
                                       { name: 'phone', type: 'String', requires_scope: 'read:User.phone' }
                                     ])

      types = FraiseQL::SchemaRegistry.instance.all_types
      type_def = types[0]

      expect(type_def[:fields].find { |f| f[:name] == 'email' }[:requires_scope]).to eq('read:User.email')
    end

    it 'supports action-based scope pattern' do
      # RED: Action patterns like read:*, write:*, admin:*
      FraiseQL::Schema.register_type('ActionPatternScopes', [
                                       { name: 'readable_field', type: 'String', requires_scope: 'read:User.*' },
                                       { name: 'writable_field', type: 'String', requires_scope: 'write:User.*' }
                                     ])

      types = FraiseQL::SchemaRegistry.instance.all_types
      type_def = types[0]

      expect(type_def[:fields].find { |f| f[:name] == 'readable_field' }[:requires_scope]).to eq('read:User.*')
      expect(type_def[:fields].find { |f| f[:name] == 'writable_field' }[:requires_scope]).to eq('write:User.*')
    end

    it 'supports global wildcard scope' do
      # RED: Global wildcard matching all scopes
      FraiseQL::Schema.register_type('GlobalWildcardScope', [
                                       { name: 'admin_override', type: 'String', requires_scope: '*' }
                                     ])

      types = FraiseQL::SchemaRegistry.instance.all_types
      type_def = types[0]

      expect(type_def[:fields][0][:requires_scope]).to eq('*')
    end
  end

  # =========================================================================
  # HAPPY PATH: JSON EXPORT (3 tests)
  # =========================================================================

  describe 'JSON export of scopes' do
    it 'exports single scope to JSON' do
      # RED: Scope must appear in JSON export
      FraiseQL::Schema.register_type('ExportTestSingleScope', [
                                       { name: 'salary', type: 'Float', requires_scope: 'read:user.salary' }
                                     ])

      json = FraiseQL::Schema.export_types(pretty: true)
      schema = JSON.parse(json)

      expect(schema).to have_key('types')
      expect(schema['types']).to have_length(1)

      salary_field = schema['types'][0]['fields'][0]
      expect(salary_field).to have_key('requires_scope')
      expect(salary_field['requires_scope']).to eq('read:user.salary')
    end

    it 'exports multiple scopes array to JSON' do
      # RED: requires_scopes array exported correctly
      FraiseQL::Schema.register_type('ExportTestMultipleScopes', [
                                       { name: 'restricted', type: 'String', requires_scopes: %w[scope1:read scope2:read] }
                                     ])

      json = FraiseQL::Schema.export_types(pretty: true)
      schema = JSON.parse(json)

      field = schema['types'][0]['fields'][0]
      expect(field).to have_key('requires_scopes')
      expect(field['requires_scopes']).to be_an(Array)
      expect(field['requires_scopes']).to have_length(2)
    end

    it 'omits scope fields for public fields in JSON' do
      # RED: Public fields should NOT have scope in JSON
      FraiseQL::Schema.register_type('ExportTestPublicField', [
                                       { name: 'id', type: 'Int' },
                                       { name: 'name', type: 'String' }
                                     ])

      json = FraiseQL::Schema.export_types(pretty: true)
      schema = JSON.parse(json)

      id_field = schema['types'][0]['fields'][0]
      expect(id_field).not_to have_key('requires_scope')
      expect(id_field).not_to have_key('requires_scopes')
    end
  end

  # =========================================================================
  # HAPPY PATH: SCOPE WITH OTHER METADATA (3 tests)
  # =========================================================================

  describe 'Scope with other field metadata' do
    it 'preserves scope alongside other field metadata' do
      # RED: Scope doesn't interfere with type, nullable, description
      FraiseQL::Schema.register_type('ScopeWithMetadata', [
                                       {
                                         name: 'salary',
                                         type: 'Float',
                                         requires_scope: 'read:user.salary',
                                         description: "User's annual salary",
                                         nullable: false
                                       }
                                     ])

      types = FraiseQL::SchemaRegistry.instance.all_types
      salary_field = types[0][:fields][0]

      expect(salary_field[:type]).to eq('Float')
      expect(salary_field[:requires_scope]).to eq('read:user.salary')
      expect(salary_field[:description]).to eq("User's annual salary")
      expect(salary_field[:nullable]).to be false
    end

    it 'works with nullable fields' do
      # RED: Scope works on nullable fields
      FraiseQL::Schema.register_type('ScopeWithNullable', [
                                       {
                                         name: 'optional_email',
                                         type: 'String',
                                         nullable: true,
                                         requires_scope: 'read:user.email'
                                       }
                                     ])

      types = FraiseQL::SchemaRegistry.instance.all_types
      email_field = types[0][:fields][0]

      expect(email_field[:nullable]).to be true
      expect(email_field[:requires_scope]).to eq('read:user.email')
    end

    it 'maintains metadata independence across multiple scoped fields' do
      # RED: Each field's metadata is independent
      FraiseQL::Schema.register_type('MetadataIndependence', [
                                       {
                                         name: 'field1',
                                         type: 'String',
                                         requires_scope: 'scope1:read',
                                         description: 'Desc 1'
                                       },
                                       {
                                         name: 'field2',
                                         type: 'String',
                                         requires_scope: 'scope2:read',
                                         description: 'Desc 2'
                                       }
                                     ])

      types = FraiseQL::SchemaRegistry.instance.all_types
      fields = types[0][:fields]

      expect(fields[0][:requires_scope]).to eq('scope1:read')
      expect(fields[0][:description]).to eq('Desc 1')
      expect(fields[1][:requires_scope]).to eq('scope2:read')
      expect(fields[1][:description]).to eq('Desc 2')
    end
  end

  # =========================================================================
  # VALIDATION: ERROR HANDLING (6 tests)
  # =========================================================================

  describe 'Scope validation and error handling' do
    it 'detects invalid scope format' do
      # RED: Invalid scopes should raise error
      expect do
        FraiseQL::Schema.register_type('InvalidScopeFormat', [
                                         { name: 'field', type: 'String', requires_scope: 'invalid_scope_no_colon' }
                                       ])
      end.to raise_error(RuntimeError)
    end

    it 'rejects empty scope string' do
      # RED: Empty string scope invalid
      expect do
        FraiseQL::Schema.register_type('EmptyScope', [
                                         { name: 'field', type: 'String', requires_scope: '' }
                                       ])
      end.to raise_error(RuntimeError)
    end

    it 'rejects empty scopes array' do
      # RED: Empty array not allowed
      expect do
        FraiseQL::Schema.register_type('EmptyScopesArray', [
                                         { name: 'field', type: 'String', requires_scopes: [] }
                                       ])
      end.to raise_error(RuntimeError)
    end

    it 'catches invalid action with hyphens' do
      # RED: Hyphens in action prefix invalid
      expect do
        FraiseQL::Schema.register_type('InvalidActionWithHyphens', [
                                         { name: 'field', type: 'String', requires_scope: 'invalid-action:resource' }
                                       ])
      end.to raise_error(RuntimeError)
    end

    it 'catches invalid resource with hyphens' do
      # RED: Hyphens in resource name invalid
      expect do
        FraiseQL::Schema.register_type('InvalidResourceWithHyphens', [
                                         { name: 'field', type: 'String', requires_scope: 'read:invalid-resource-name' }
                                       ])
      end.to raise_error(RuntimeError)
    end

    it 'rejects conflicting both scope and scopes' do
      # RED: Can't have both on same field
      expect do
        FraiseQL::Schema.register_type('ConflictingScopeAndScopes', [
                                         {
                                           name: 'field',
                                           type: 'String',
                                           requires_scope: 'read:user.email',
                                           requires_scopes: %w[admin:read auditor:read]
                                         }
                                       ])
      end.to raise_error(RuntimeError)
    end
  end
end
