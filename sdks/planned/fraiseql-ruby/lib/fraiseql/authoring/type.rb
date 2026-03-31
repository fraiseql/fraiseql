# frozen_string_literal: true

module FraiseQL
  # Mixin for declaring FraiseQL GraphQL types in Ruby classes.
  #
  # Usage:
  #   class User
  #     include FraiseQL::Type
  #     fraiseql_type_name 'User'
  #     fraiseql_field :id, 'ID!', description: 'Unique identifier'
  #   end
  module Type
    def self.included(base)
      base.extend(ClassMethods)
    end

    # Class-level DSL methods added when FraiseQL::Type is included.
    module ClassMethods
      # Get or set the GraphQL type name.
      #
      # @param name [String, nil] the type name to set, or nil to read
      # @return [String] the configured type name
      def fraiseql_type_name(name = nil)
        if name
          @fraiseql_type_name = name
        else
          @fraiseql_type_name
        end
      end

      # Declare a field on this type.
      #
      # @param name [Symbol] field name
      # @param graphql_type [String] GraphQL type string (e.g. "String!", "ID")
      # @param description [String, nil] field description
      # @param required [Boolean] whether the field is required (not currently used in schema output)
      # @param deprecated [Boolean] whether the field is deprecated
      def fraiseql_field(name, graphql_type, description: nil, required: true, deprecated: false)
        @fraiseql_fields ||= []
        field = { name: name.to_s, type: graphql_type }
        field[:description] = description if description
        field[:deprecated] = true if deprecated
        @fraiseql_fields << field
      end

      # Serialize this type to a FraiseQL schema hash.
      #
      # @return [Hash] schema representation
      def to_fraiseql_schema
        {
          name: @fraiseql_type_name,
          fields: (@fraiseql_fields || []).dup
        }
      end
    end
  end
end
