# frozen_string_literal: true

# Generate parity schema for cross-SDK comparison.
#
# Usage:
#   SCHEMA_OUTPUT_FILE=/tmp/schema_ruby.json bundle exec rspec spec/generate_parity_schema_spec.rb

require "json"

RSpec.describe "Parity schema generation" do
  it "generates the canonical parity schema" do
    schema = {
      types: [
        {
          name: "User",
          sql_source: "v_user",
          fields: [
            { name: "id", type: "ID", nullable: false },
            { name: "email", type: "String", nullable: false },
            { name: "name", type: "String", nullable: false }
          ]
        },
        {
          name: "Order",
          sql_source: "v_order",
          fields: [
            { name: "id", type: "ID", nullable: false },
            { name: "total", type: "Float", nullable: false }
          ]
        },
        {
          name: "UserNotFound",
          sql_source: "v_user_not_found",
          is_error: true,
          fields: [
            { name: "message", type: "String", nullable: false },
            { name: "code", type: "String", nullable: false }
          ]
        }
      ],
      queries: [
        {
          name: "users",
          return_type: "User",
          returns_list: true,
          nullable: false,
          sql_source: "v_user",
          arguments: []
        },
        {
          name: "tenantOrders",
          return_type: "Order",
          returns_list: true,
          nullable: false,
          sql_source: "v_order",
          inject_params: { tenant_id: "jwt:tenant_id" },
          cache_ttl_seconds: 300,
          requires_role: "admin",
          arguments: []
        }
      ],
      mutations: [
        {
          name: "createUser",
          return_type: "User",
          sql_source: "fn_create_user",
          operation: "insert",
          arguments: [
            { name: "email", type: "String", nullable: false },
            { name: "name", type: "String", nullable: false }
          ]
        },
        {
          name: "placeOrder",
          return_type: "Order",
          sql_source: "fn_place_order",
          operation: "insert",
          inject_params: { user_id: "jwt:sub" },
          invalidates_views: ["v_order_summary"],
          invalidates_fact_tables: ["tf_sales"],
          arguments: []
        }
      ]
    }

    json = JSON.pretty_generate(schema)
    output_file = ENV.fetch("SCHEMA_OUTPUT_FILE", nil)

    if output_file && !output_file.empty?
      File.write(output_file, json)
    else
      puts json
    end
  end
end
