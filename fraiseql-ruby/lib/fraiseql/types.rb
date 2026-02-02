module FraiseQL
  # Type and field definitions for GraphQL schema
  module Types
    # Represents a GraphQL field in a type
    class FieldInfo
      attr_accessor :name, :type, :nullable, :description, :requires_scope, :requires_scopes

      def initialize(name:, type:, nullable: false, description: nil, requires_scope: nil, requires_scopes: nil)
        @name = name
        @type = type
        @nullable = nullable
        @description = description
        @requires_scope = requires_scope
        @requires_scopes = requires_scopes
      end

      # Support hash-like access for compatibility
      def [](key)
        case key
        when :name then @name
        when :type then @type
        when :nullable then @nullable
        when :description then @description
        when :requires_scope then @requires_scope
        when :requires_scopes then @requires_scopes
        else nil
        end
      end

      def to_h
        {
          name: @name,
          type: @type,
          nullable: @nullable
        }.tap do |h|
          h[:description] = @description if @description
          h[:requires_scope] = @requires_scope if @requires_scope
          h[:requires_scopes] = @requires_scopes if @requires_scopes
        end
      end
    end

    # Represents a GraphQL type
    class TypeDefinition
      attr_accessor :name, :fields, :description

      def initialize(name:, fields: [], description: nil)
        @name = name
        @fields = fields
        @description = description
      end

      def to_h
        {
          name: @name,
          fields: @fields
        }.tap do |h|
          h[:description] = @description if @description
        end
      end
    end
  end
end
