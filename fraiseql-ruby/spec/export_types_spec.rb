require 'spec_helper'
require 'json'

describe 'Export Types - Minimal Schema Export' do
    before(:each) do
      FraiseQL::Schema.reset
    end

    it 'exports minimal schema with single type' do
      # Register a single type
      FraiseQL::Schema.register_type('User', [
        { name: 'id', type: 'ID', nullable: false },
        { name: 'name', type: 'String', nullable: false },
        { name: 'email', type: 'String', nullable: false }
      ], 'User in the system')

      # Export minimal types
      schema_json = FraiseQL::Schema.export_types(true)
      parsed = JSON.parse(schema_json)

      # Should have types section
      expect(parsed).to have_key('types')
      expect(parsed['types']).to be_an(Array)
      expect(parsed['types'].length).to eq(1)

      # Should NOT have queries, mutations, observers, etc.
      expect(parsed).not_to have_key('queries')
      expect(parsed).not_to have_key('mutations')
      expect(parsed).not_to have_key('observers')
      expect(parsed).not_to have_key('authz_policies')

      # Verify User type
      user_type = parsed['types'].first
      expect(user_type['name']).to eq('User')
      expect(user_type['description']).to eq('User in the system')
    end

    it 'exports minimal schema with multiple types' do
      # Register multiple types
      FraiseQL::Schema.register_type('User', [
        { name: 'id', type: 'ID', nullable: false },
        { name: 'name', type: 'String', nullable: false }
      ])

      FraiseQL::Schema.register_type('Post', [
        { name: 'id', type: 'ID', nullable: false },
        { name: 'title', type: 'String', nullable: false },
        { name: 'authorId', type: 'ID', nullable: false }
      ])

      # Export minimal
      schema_json = FraiseQL::Schema.export_types(true)
      parsed = JSON.parse(schema_json)

      # Check types count
      expect(parsed['types'].length).to eq(2)

      # Verify both types present
      type_names = parsed['types'].map { |t| t['name'] }
      expect(type_names).to include('User', 'Post')
    end

    it 'does not include queries in minimal export' do
      # Register type and query
      FraiseQL::Schema.register_type('User', [
        { name: 'id', type: 'ID', nullable: false }
      ])

      # (In a real SDK, queries would be registered here)
      # But minimal export should only have types

      # Export minimal
      schema_json = FraiseQL::Schema.export_types(true)
      parsed = JSON.parse(schema_json)

      # Should have types
      expect(parsed).to have_key('types')

      # Should NOT have queries
      expect(parsed).not_to have_key('queries')
      expect(parsed).not_to have_key('mutations')
    end

    it 'exports compact format when pretty is false' do
      FraiseQL::Schema.register_type('User', [
        { name: 'id', type: 'ID', nullable: false }
      ])

      # Export compact
      schema_json = FraiseQL::Schema.export_types(false)

      # Should be valid JSON
      parsed = JSON.parse(schema_json)
      expect(parsed).to have_key('types')

      # Compact JSON should be smaller than pretty-printed
      compact = FraiseQL::Schema.export_types(false)
      pretty = FraiseQL::Schema.export_types(true)
      expect(compact.length).to be < pretty.length
    end

    it 'exports pretty format when pretty is true' do
      FraiseQL::Schema.register_type('User', [
        { name: 'id', type: 'ID', nullable: false }
      ])

      # Export pretty
      schema_json = FraiseQL::Schema.export_types(true)

      # Should contain newlines (pretty format)
      expect(schema_json).to include("\n")

      # Should be valid JSON
      parsed = JSON.parse(schema_json)
      expect(parsed).to have_key('types')
    end

    it 'exports types to file' do
      FraiseQL::Schema.register_type('User', [
        { name: 'id', type: 'ID', nullable: false },
        { name: 'name', type: 'String', nullable: false }
      ])

      # Export to temporary file
      tmp_file = '/tmp/fraiseql_types_test.json'

      # Remove file if exists
      File.delete(tmp_file) if File.exist?(tmp_file)

      # Export to file
      FraiseQL::Schema.export_types_file(tmp_file)

      # Verify file exists and is valid JSON
      expect(File.exist?(tmp_file)).to be true

      content = File.read(tmp_file)
      parsed = JSON.parse(content)

      expect(parsed).to have_key('types')
      expect(parsed['types'].length).to eq(1)

      # Cleanup
      File.delete(tmp_file)
    end

    it 'handles empty schema gracefully' do
      # Export with no types registered
      schema_json = FraiseQL::Schema.export_types(true)
      parsed = JSON.parse(schema_json)

      # Should still have types key (as empty array)
      expect(parsed).to have_key('types')
      expect(parsed['types']).to be_an(Array)
      expect(parsed['types'].length).to eq(0)
    end
  end
