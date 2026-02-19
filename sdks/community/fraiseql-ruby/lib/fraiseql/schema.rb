require 'json'
require_relative 'registry'
require_relative 'types'

module FraiseQL
  # Schema management and type export for TOML-based workflow
  class Schema
    # Export types to minimal JSON (TOML workflow)
    # The pretty parameter controls formatting (true = pretty-printed, false = compact)
    def self.export_types(pretty = true)
      registry = SchemaRegistry.instance
      types = registry.get_types

      # Convert FieldInfo objects to hashes for JSON export
      exported_types = types.map do |type_def|
        {
          name: type_def[:name],
          fields: type_def[:fields].map { |field| field.to_h },
          description: type_def[:description]
        }.tap { |h| h.delete(:description) if h[:description].nil? }
      end

      # Build minimal schema with only types
      minimal_schema = {
        types: exported_types
      }

      if pretty
        JSON.pretty_generate(minimal_schema)
      else
        JSON.generate(minimal_schema)
      end
    end

    # Export types to a file
    def self.export_types_file(output_path)
      types_json = export_types(true)

      File.write(output_path, types_json)

      # Print summary
      registry = SchemaRegistry.instance
      types_count = registry.get_types.length

      puts "✅ Types exported to #{output_path}"
      puts "   Types: #{types_count}"
      puts ""
      puts "🎯 Next steps:"
      puts "   1. fraiseql compile fraiseql.toml --types #{output_path}"
      puts "   2. This merges types with TOML configuration"
      puts "   3. Result: schema.compiled.json with types + all config"
    end

    # Register a type
    def self.register_type(name, fields_array, description = nil)
      # Validate and extract scopes from fields
      validated_fields = fields_array.map do |field_config|
        validated_field = field_config.dup

        # Validate scope if present
        if field_config[:requires_scope]
          validate_scope(field_config[:requires_scope], name, field_config[:name])
        end

        # Validate scopes array if present
        if field_config[:requires_scopes]
          if field_config[:requires_scopes].empty?
            raise RuntimeError, "Field #{name}.#{field_config[:name]} has empty scopes array"
          end
          field_config[:requires_scopes].each do |scope|
            if scope.empty?
              raise RuntimeError, "Field #{name}.#{field_config[:name]} has empty scope in scopes array"
            end
            validate_scope(scope, name, field_config[:name])
          end
        end

        # Ensure not both scope and scopes
        if field_config[:requires_scope] && field_config[:requires_scopes]
          raise RuntimeError, "Field #{name}.#{field_config[:name]} cannot have both requires_scope and requires_scopes"
        end

        validated_field
      end

      registry = SchemaRegistry.instance
      registry.register_type(name, validated_fields, description)
    end

    # Reset registry (useful for testing)
    def self.reset
      registry = SchemaRegistry.instance
      registry.reset
    end

    # Get all registered types
    def self.get_types
      registry = SchemaRegistry.instance
      registry.get_types
    end

    private

    # Validate scope format: action:resource
    # Valid patterns:
    # - * (global wildcard)
    # - action:resource (read:user.email, write:User.salary)
    # - action:* (admin:*, read:*)
    def self.validate_scope(scope, type_name, field_name)
      if scope.empty?
        raise RuntimeError, "Field #{type_name}.#{field_name} has empty scope"
      end

      # Global wildcard is always valid
      return if scope == '*'

      # Must contain at least one colon
      unless scope.include?(':')
        raise RuntimeError, "Field #{type_name}.#{field_name} has invalid scope '#{scope}' (missing colon)"
      end

      parts = scope.split(':', 2)
      if parts.size != 2
        raise RuntimeError, "Field #{type_name}.#{field_name} has invalid scope '#{scope}'"
      end

      action = parts[0]
      resource = parts[1]

      # Validate action: [a-zA-Z_][a-zA-Z0-9_]*
      unless valid_action?(action)
        raise RuntimeError, "Field #{type_name}.#{field_name} has invalid action in scope '#{scope}' (must be alphanumeric + underscore)"
      end

      # Validate resource: [a-zA-Z_][a-zA-Z0-9_.]*|*
      unless valid_resource?(resource)
        raise RuntimeError, "Field #{type_name}.#{field_name} has invalid resource in scope '#{scope}' (must be alphanumeric + underscore + dot, or *)"
      end
    end

    # Check if action matches [a-zA-Z_][a-zA-Z0-9_]*
    def self.valid_action?(action)
      return false if action.empty?

      # First character must be letter or underscore
      first_char = action[0]
      return false unless first_char.match?(/[a-zA-Z_]/)

      # Rest must be letters, digits, or underscores
      action[1..-1].each_char do |ch|
        return false unless ch.match?(/[a-zA-Z0-9_]/)
      end

      true
    end

    # Check if resource matches [a-zA-Z_][a-zA-Z0-9_.]*|*
    def self.valid_resource?(resource)
      return true if resource == '*'
      return false if resource.empty?

      # First character must be letter or underscore
      first_char = resource[0]
      return false unless first_char.match?(/[a-zA-Z_]/)

      # Rest must be letters, digits, underscores, or dots
      resource[1..-1].each_char do |ch|
        return false unless ch.match?(/[a-zA-Z0-9_.]/)
      end

      true
    end
  end
end
