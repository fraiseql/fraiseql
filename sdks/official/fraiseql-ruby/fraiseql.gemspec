# frozen_string_literal: true

require_relative "lib/fraiseql/version"

Gem::Specification.new do |spec|
  spec.name = "fraiseql"
  spec.version = FraiseQL::VERSION
  spec.authors = ["FraiseQL Team"]
  spec.summary = "FraiseQL Ruby SDK - Compiled GraphQL client"
  spec.description = "Ruby client for FraiseQL GraphQL servers with AI framework integrations"
  spec.homepage = "https://github.com/fraiseql/fraiseql"
  spec.license = "MIT"
  spec.required_ruby_version = ">= 3.1"

  spec.metadata = { "rubygems_mfa_required" => "true" }

  spec.files = Dir["lib/**/*.rb", "LICENSE", "README.md"]
  spec.require_paths = ["lib"]
end
