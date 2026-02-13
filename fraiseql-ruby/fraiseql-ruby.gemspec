Gem::Specification.new do |spec|
  spec.name          = "fraiseql-ruby"
  spec.version       = "1.0.0"
  spec.authors       = ["FraiseQL Contributors"]
  spec.email         = ["contact@fraiseql.dev"]

  spec.summary       = "FraiseQL Ruby - 100% Feature Parity GraphQL Schema Authoring"
  spec.description   = <<-DESC
    FraiseQL Ruby provides declarative, type-safe GraphQL schema definitions with:
    - Advanced authorization and security features
    - Role-based access control (RBAC)
    - Attribute-based access control (ABAC)
    - Authorization policies
    - 100% feature parity with Python, Java, Go, PHP, TypeScript, and Node.js
  DESC

  spec.homepage      = "https://github.com/fraiseql/fraiseql"
  spec.license       = "Apache-2.0"

  spec.files         = Dir.glob("lib/**/*.rb") + Dir.glob("spec/**/*.rb") +
                       ["README.md", "RUBY_FEATURE_PARITY.md", "Gemfile", ".gitignore"]
  spec.bindir        = "bin"
  spec.require_paths = ["lib"]

  spec.required_ruby_version = ">= 2.7.0"

  spec.add_development_dependency "bundler", "~> 2.0"
  spec.add_development_dependency "rspec", "~> 3.12"
  spec.add_development_dependency "rspec-its", "~> 1.3"
  spec.add_development_dependency "rubocop", "~> 1.40"
  spec.add_development_dependency "rake", "~> 13.0"
end
