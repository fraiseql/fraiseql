# frozen_string_literal: true

module FraiseQL
  module Type
    def self.included(base)
      base.extend(ClassMethods)
      base.instance_variable_set(:@fraiseql_fields, {})
      base.instance_variable_set(:@fraiseql_type_name, base.name.split("::").last)
    end

    module ClassMethods
      def fraiseql_field(name, type, description: nil, deprecated: false, required: true)
        @fraiseql_fields[name] = { type: type, description: description, deprecated: deprecated, required: required }
      end

      def fraiseql_type_name(name = nil)
        @fraiseql_type_name = name if name
        @fraiseql_type_name
      end

      def to_fraiseql_schema
        {
          name: @fraiseql_type_name,
          fields: @fraiseql_fields.map { |fname, fmeta|
            { name: fname.to_s, type: fmeta[:type].to_s }.tap { |f|
              f[:description] = fmeta[:description] if fmeta[:description]
              f[:deprecated] = true if fmeta[:deprecated]
            }
          }
        }
      end
    end
  end
end
