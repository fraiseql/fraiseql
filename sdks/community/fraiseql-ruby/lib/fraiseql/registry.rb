require 'singleton'

module FraiseQL
  # Singleton registry for collecting types
  class SchemaRegistry
    include Singleton

    attr_reader :types

    def initialize
      @types = {}
      @mutex = Mutex.new
    end

    # Register a type with the schema registry
    def register_type(name, fields_array, description = nil)
      @mutex.synchronize do
        @types[name] = {
          name: name,
          fields: fields_array,
          description: description
        }
      end
    end

    # Get all registered types as hashes with FieldInfo objects
    def get_types
      @mutex.synchronize do
        @types.values.map { |type_def| convert_to_field_info(type_def) }
      end
    end

    # Reset the registry (useful for testing)
    def reset
      @mutex.synchronize do
        @types = {}
      end
    end

    private

    # Convert raw type definition to one with FieldInfo objects
    def convert_to_field_info(type_def)
      fields = type_def[:fields].map do |field_config|
        Types::FieldInfo.new(
          name: field_config[:name],
          type: field_config[:type],
          nullable: field_config[:nullable] || false,
          description: field_config[:description],
          requires_scope: field_config[:requires_scope],
          requires_scopes: field_config[:requires_scopes]
        )
      end

      {
        name: type_def[:name],
        fields: fields,
        description: type_def[:description]
      }
    end
  end
end
