# frozen_string_literal: true

require 'json'
require_relative 'registry'
require_relative 'types'

module FraiseQL
  # Schema management and type export for TOML-based workflow
  class Schema
    # Export types to minimal JSON (TOML workflow)
    def self.export_types(pretty: true)
      registry = SchemaRegistry.instance
      types = registry.all_types

      exported_types = types.map do |type_def|
        {
          name: type_def[:name],
          fields: type_def[:fields].map(&:to_h),
          description: type_def[:description]
        }.tap { |h| h.delete(:description) if h[:description].nil? }
      end

      minimal_schema = { types: exported_types }

      pretty ? JSON.pretty_generate(minimal_schema) : JSON.generate(minimal_schema)
    end

    # Export types to a file
    def self.export_types_file(output_path)
      types_json = export_types(pretty: true)
      File.write(output_path, types_json)
      print_export_summary(output_path)
    end

    # Register a type
    def self.register_type(name, fields_array, description = nil)
      validated_fields = fields_array.map { |fc| validate_field(fc, name) }
      SchemaRegistry.instance.register_type(name, validated_fields, description)
    end

    # Reset registry (useful for testing)
    def self.reset
      SchemaRegistry.instance.reset
    end

    # Get all registered types
    def self.all_types
      SchemaRegistry.instance.all_types
    end

    def self.print_export_summary(output_path)
      types_count = SchemaRegistry.instance.all_types.length
      puts "✅ Types exported to #{output_path}"
      puts "   Types: #{types_count}"
      puts ''
      puts '🎯 Next steps:'
      puts "   1. fraiseql compile fraiseql.toml --types #{output_path}"
      puts '   2. This merges types with TOML configuration'
      puts '   3. Result: schema.compiled.json with types + all config'
    end

    def self.validate_field(field_config, type_name)
      validate_requires_scope(field_config, type_name)
      validate_requires_scopes(field_config, type_name)
      raise_if_both_scope_and_scopes(field_config, type_name)
      field_config.dup
    end

    def self.validate_requires_scope(field_config, type_name)
      return unless field_config[:requires_scope]

      validate_scope(field_config[:requires_scope], type_name, field_config[:name])
    end

    def self.validate_requires_scopes(field_config, type_name)
      return unless field_config[:requires_scopes]

      fname = field_config[:name]
      raise "Field #{type_name}.#{fname} has empty scopes array" if field_config[:requires_scopes].empty?

      field_config[:requires_scopes].each do |scope|
        raise "Field #{type_name}.#{fname} has empty scope in scopes array" if scope.empty?

        validate_scope(scope, type_name, fname)
      end
    end

    def self.raise_if_both_scope_and_scopes(field_config, type_name)
      return unless field_config[:requires_scope] && field_config[:requires_scopes]

      raise "Field #{type_name}.#{field_config[:name]} cannot have both requires_scope and requires_scopes"
    end

    # Validate scope format: action:resource
    def self.validate_scope(scope, type_name, field_name)
      raise "Field #{type_name}.#{field_name} has empty scope" if scope.empty?
      return if scope == '*'

      raise "Field #{type_name}.#{field_name} has invalid scope '#{scope}' (missing colon)" unless scope.include?(':')

      parts = scope.split(':', 2)
      action = parts[0]
      resource = parts[1]

      unless valid_action?(action)
        raise "Field #{type_name}.#{field_name} has invalid action in scope '#{scope}' " \
              '(must be alphanumeric + underscore)'
      end

      return if valid_resource?(resource)

      raise "Field #{type_name}.#{field_name} has invalid resource in scope '#{scope}' " \
            '(must be alphanumeric + underscore + dot, or *)'
    end

    def self.valid_action?(action)
      return false if action.empty?

      action.match?(/\A[a-zA-Z_][a-zA-Z0-9_]*\z/)
    end

    def self.valid_resource?(resource)
      return true if resource == '*'
      return false if resource.empty?

      resource.match?(/\A[a-zA-Z_][a-zA-Z0-9_.]*\z/)
    end

    private_class_method :print_export_summary, :validate_field, :validate_requires_scope,
                         :validate_requires_scopes, :raise_if_both_scope_and_scopes,
                         :validate_scope, :valid_action?, :valid_resource?
  end
end
