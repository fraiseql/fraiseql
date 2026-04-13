# frozen_string_literal: true

require_relative "crud_generator"

module FraiseQL
  module Type
    def self.included(base)
      base.extend(ClassMethods)
      base.instance_variable_set(:@fraiseql_fields, {})
      base.instance_variable_set(:@fraiseql_type_name, base.name.split("::").last)
      base.instance_variable_set(:@fraiseql_crud, false)
      base.instance_variable_set(:@fraiseql_cascade, false)
      base.instance_variable_set(:@fraiseql_sql_source, nil)
    end

    module ClassMethods
      def fraiseql_field(name, type, description: nil, deprecated: false, required: true)
        @fraiseql_fields[name] = { type: type, description: description, deprecated: deprecated, required: required }
      end

      def fraiseql_type_name(name = nil)
        @fraiseql_type_name = name if name
        @fraiseql_type_name
      end

      def fraiseql_crud(enabled = true)
        @fraiseql_crud = enabled
      end

      def fraiseql_cascade(enabled = true)
        @fraiseql_cascade = enabled
      end

      def fraiseql_sql_source(source = nil)
        if source
          @fraiseql_sql_source = source
        else
          @fraiseql_sql_source || "v_#{CrudGenerator.pascal_to_snake(fraiseql_type_name || name)}"
        end
      end

      def fraiseql_crud_enabled?
        @fraiseql_crud
      end

      def fraiseql_cascade_enabled?
        @fraiseql_cascade
      end

      def to_fraiseql_schema
        {
          name: @fraiseql_type_name,
          sql_source: fraiseql_sql_source,
          fields: @fraiseql_fields.map { |fname, fmeta|
            { name: CrudGenerator.snake_to_camel(fname.to_s), type: fmeta[:type].to_s }.tap { |f|
              f[:description] = fmeta[:description] if fmeta[:description]
              f[:deprecated] = true if fmeta[:deprecated]
            }
          }
        }
      end

      def to_fraiseql_crud
        return nil unless @fraiseql_crud

        fields = @fraiseql_fields.map do |fname, fmeta|
          { name: fname.to_s, type: fmeta[:type].to_s, nullable: !fmeta[:required] }
        end

        CrudGenerator.generate(
          type_name: @fraiseql_type_name,
          fields: fields,
          sql_source: fraiseql_sql_source,
          cascade: @fraiseql_cascade
        )
      end
    end
  end
end
