# frozen_string_literal: true

require 'spec_helper'

RSpec.describe FraiseQL::Type do
  let(:user_class) do
    Class.new do
      include FraiseQL::Type

      fraiseql_type_name 'User'
      fraiseql_field :id, 'ID!', description: 'Unique identifier'
      fraiseql_field :name, 'String!', description: 'Display name'
      fraiseql_field :email, 'String', required: false
      fraiseql_field :legacy_field, 'String', deprecated: true
    end
  end

  describe '.fraiseql_type_name' do
    it 'returns the configured type name' do
      expect(user_class.fraiseql_type_name).to eq('User')
    end
  end

  describe '.to_fraiseql_schema' do
    subject(:schema) { user_class.to_fraiseql_schema }

    it 'includes the type name' do
      expect(schema[:name]).to eq('User')
    end

    it 'includes all fields' do
      field_names = schema[:fields].map { |f| f[:name] }
      expect(field_names).to contain_exactly('id', 'name', 'email', 'legacy_field')
    end

    it 'includes descriptions' do
      id_field = schema[:fields].find { |f| f[:name] == 'id' }
      expect(id_field[:description]).to eq('Unique identifier')
    end

    it 'marks deprecated fields' do
      legacy = schema[:fields].find { |f| f[:name] == 'legacy_field' }
      expect(legacy[:deprecated]).to be(true)
    end

    it 'does not add deprecated key for non-deprecated fields' do
      id_field = schema[:fields].find { |f| f[:name] == 'id' }
      expect(id_field).not_to have_key(:deprecated)
    end
  end
end
