# frozen_string_literal: true

Gem::Specification.new do |spec|
  spec.name    = 'fraiseql'
  spec.version = FraiseQL::VERSION
  spec.authors = ['FraiseQL Team']
  spec.email   = ['team@fraiseql.dev']
  spec.summary = 'FraiseQL schema authoring SDK for Ruby'
  spec.description = 'Define GraphQL schemas with Ruby decorators for the FraiseQL compiled execution engine'
  spec.homepage    = 'https://github.com/fraiseql/fraiseql'
  spec.license     = 'MIT'

  spec.required_ruby_version = '>= 3.2'

  spec.metadata = {
    'rubygems_mfa_required' => 'true',
    'source_code_uri' => 'https://github.com/fraiseql/fraiseql',
    'changelog_uri' => 'https://github.com/fraiseql/fraiseql/blob/dev/CHANGELOG.md'
  }

  spec.files = Dir['lib/**/*', 'README.md']
end
