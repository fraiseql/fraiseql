# frozen_string_literal: true

# Generate parity schema for cross-SDK comparison.
#
# Produces the canonical parity-schema JSON compatible with the Python
# reference generator and compare_schemas.py.
#
# Usage:
#   ruby test/generate_parity_schema.rb
#
# When SCHEMA_OUTPUT_FILE is set the JSON is written to that path instead
# of stdout:
#   SCHEMA_OUTPUT_FILE=/tmp/schema_ruby.json ruby test/generate_parity_schema.rb

require "json"

# ── Types ──────────────────────────────────────────────────────────────────

def make_field(name, type, nullable)
  { "name" => name, "type" => type, "nullable" => nullable }
end

def make_argument(name, type, nullable)
  { "name" => name, "type" => type, "nullable" => nullable }
end

types = [
  {
    "name" => "User",
    "sql_source" => "v_user",
    "fields" => [
      make_field("id",    "ID",     false),
      make_field("email", "String", false),
      make_field("name",  "String", false),
    ],
  },
  {
    "name" => "Order",
    "sql_source" => "v_order",
    "fields" => [
      make_field("id",    "ID",    false),
      make_field("total", "Float", false),
    ],
  },
  {
    "name" => "UserNotFound",
    "sql_source" => "v_user_not_found",
    "is_error" => true,
    "fields" => [
      make_field("message", "String", false),
      make_field("code",    "String", false),
    ],
  },
]

# ── Queries ────────────────────────────────────────────────────────────────

queries = [
  {
    "name"         => "users",
    "return_type"  => "User",
    "returns_list" => true,
    "nullable"     => false,
    "sql_source"   => "v_user",
    "arguments"    => [],
  },
  {
    "name"              => "tenantOrders",
    "return_type"       => "Order",
    "returns_list"      => true,
    "nullable"          => false,
    "sql_source"        => "v_order",
    "inject_params"     => { "tenant_id" => "jwt:tenant_id" },
    "cache_ttl_seconds" => 300,
    "requires_role"     => "admin",
    "arguments"         => [],
  },
]

# ── Mutations ──────────────────────────────────────────────────────────────

mutations = [
  {
    "name"        => "createUser",
    "return_type" => "User",
    "sql_source"  => "fn_create_user",
    "operation"   => "insert",
    "arguments"   => [
      make_argument("email", "String", false),
      make_argument("name",  "String", false),
    ],
  },
  {
    "name"                    => "placeOrder",
    "return_type"             => "Order",
    "sql_source"              => "fn_place_order",
    "operation"               => "insert",
    "inject_params"           => { "user_id" => "jwt:sub" },
    "invalidates_views"       => ["v_order_summary"],
    "invalidates_fact_tables" => ["tf_sales"],
    "arguments"               => [],
  },
]

# ── Output ─────────────────────────────────────────────────────────────────

schema = { "types" => types, "queries" => queries, "mutations" => mutations }
json = JSON.pretty_generate(schema)

output_file = ENV["SCHEMA_OUTPUT_FILE"]
if output_file && !output_file.empty?
  File.write(output_file, json)
else
  puts json
end
