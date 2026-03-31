# frozen_string_literal: true

# Generate parity schema for cross-SDK comparison.
#
# Usage:
#   SCHEMA_OUTPUT_FILE=/tmp/schema_ruby.json bundle exec rspec spec/generate_parity_schema_spec.rb

require 'json'

module ParitySchemaHelper
  def self.canonical_schema # rubocop:disable Metrics/MethodLength
    {
      types: [
        {
          name: 'User',
          sql_source: 'v_user',
          fields: [
            { name: 'id', type: 'ID', nullable: false },
            { name: 'email', type: 'String', nullable: false },
            { name: 'name', type: 'String', nullable: false }
          ]
        },
        {
          name: 'Order',
          sql_source: 'v_order',
          fields: [
            { name: 'id', type: 'ID', nullable: false },
            { name: 'total', type: 'Float', nullable: false }
          ]
        },
        {
          name: 'UserNotFound',
          sql_source: 'v_user_not_found',
          is_error: true,
          fields: [
            { name: 'message', type: 'String', nullable: false },
            { name: 'code', type: 'String', nullable: false }
          ]
        }
      ],
      queries: [
        {
          name: 'users',
          return_type: 'User',
          returns_list: true,
          nullable: false,
          sql_source: 'v_user',
          arguments: []
        },
        {
          name: 'tenantOrders',
          return_type: 'Order',
          returns_list: true,
          nullable: false,
          sql_source: 'v_order',
          inject_params: { tenant_id: 'jwt:tenant_id' },
          cache_ttl_seconds: 300,
          requires_role: 'admin',
          arguments: []
        }
      ],
      mutations: [
        {
          name: 'createUser',
          return_type: 'User',
          sql_source: 'fn_create_user',
          operation: 'insert',
          arguments: [
            { name: 'email', type: 'String', nullable: false },
            { name: 'name', type: 'String', nullable: false }
          ]
        },
        {
          name: 'placeOrder',
          return_type: 'Order',
          sql_source: 'fn_place_order',
          operation: 'insert',
          inject_params: { user_id: 'jwt:sub' },
          invalidates_views: ['v_order_summary'],
          invalidates_fact_tables: ['tf_sales'],
          arguments: []
        }
      ]
    }
  end
end

RSpec.describe ParitySchemaHelper do
  it 'generates the canonical parity schema' do
    json = JSON.pretty_generate(described_class.canonical_schema)

    expect(json).not_to be_empty

    output_file = ENV.fetch('SCHEMA_OUTPUT_FILE', nil)
    File.write(output_file, json) if output_file && !output_file.empty?
  end
end
