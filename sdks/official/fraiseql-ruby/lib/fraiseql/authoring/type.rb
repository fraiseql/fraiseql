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
      # `deprecated` accepts `true` for "deprecated, no stated reason" or a String reason.
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

      # The type in the intermediate format, ready to be placed in a `types` array.
      #
      # `nullable` is required by the compiler and has no serde default, so omitting it
      # made every export fail with `missing field \`nullable\`` — while the sibling
      # `to_fraiseql_crud` in this same file built its field list correctly, so the SDK
      # contradicted itself (#854).
      #
      # Field names are snake_case here to match `to_fraiseql_crud`. They used to be
      # camelCased on this path only, so a type's fields and its generated CRUD arguments
      # disagreed about the name of the same column.
      #
      # `deprecated` is emitted as `{ reason: ... }`, the shape `IntermediateField` reads.
      # It used to be dropped here, correctly at the time: the compiler had no such member
      # and denied unknown fields. #1025 added it, so the reason now reaches the compiled
      # schema and surfaces through introspection as `isDeprecated` / `deprecationReason`.
      #
      # `true` means deprecated with no stated reason, which the compiler models as an
      # absent `reason`; `false` drops the key rather than emitting an empty deprecation.
      def to_fraiseql_schema
        {
          name: @fraiseql_type_name,
          sql_source: fraiseql_sql_source,
          fields: @fraiseql_fields.map { |fname, fmeta|
            { name: fname.to_s, type: fmeta[:type].to_s, nullable: !fmeta[:required] }.tap { |f|
              f[:description] = fmeta[:description] if fmeta[:description]
              f[:deprecated] = deprecation_of(fmeta[:deprecated]) if fmeta[:deprecated]
            }
          }
        }
      end

      # `true` -> no stated reason; a String -> that reason.
      def deprecation_of(deprecated)
        deprecated.is_a?(String) ? { reason: deprecated } : {}
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
