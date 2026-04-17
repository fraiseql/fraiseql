# frozen_string_literal: true

# FraiseQL Ruby SDK - Type definitions and schema export for TOML-based workflow
#
# This minimal SDK generates types.json that pairs with fraiseql.toml configuration.
# All operational config (queries, mutations, federation, security, observers) is defined in TOML.

require_relative 'fraiseql/types'
require_relative 'fraiseql/registry'
require_relative 'fraiseql/schema'

# FraiseQL Ruby SDK — schema authoring and type export.
module FraiseQL
  VERSION = '2.0.0'

  # Convenience methods for schema management
  def self.register_type(name, fields, description = nil)
    Schema.register_type(name, fields, description)
  end

  def self.export_types(pretty: true)
    Schema.export_types(pretty: pretty)
  end

  def self.export_types_file(path)
    Schema.export_types_file(path)
  end

  def self.reset
    Schema.reset
  end
end
