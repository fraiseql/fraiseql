require 'spec_helper'
require_relative '../lib/fraiseql/security'

describe FraiseQL::Security do
  describe 'AuthorizeBuilder' do
    it 'creates authorization rule builder' do
      config = FraiseQL::Security::AuthorizeBuilder.create
        .rule("isOwner($context.userId, $field.ownerId)")
        .description("Ensures users can only access their own notes")
        .build

      expect(config.rule).to eq("isOwner($context.userId, $field.ownerId)")
      expect(config.description).to eq("Ensures users can only access their own notes")
    end

    it 'creates authorization with policy reference' do
      config = FraiseQL::Security::AuthorizeBuilder.create
        .policy("piiAccess")
        .description("References the piiAccess policy")
        .build

      expect(config.policy).to eq("piiAccess")
      expect(config.cacheable).to be true
    end

    it 'creates authorization with error message' do
      config = FraiseQL::Security::AuthorizeBuilder.create
        .rule("hasRole($context, 'admin')")
        .error_message("Only administrators can access this resource")
        .build

      expect(config.error_message).to eq("Only administrators can access this resource")
    end

    it 'creates recursive authorization' do
      config = FraiseQL::Security::AuthorizeBuilder.create
        .rule("canAccessNested($context)")
        .recursive(true)
        .description("Recursively applies to nested types")
        .build

      expect(config.recursive).to be true
    end

    it 'creates operation-specific authorization' do
      config = FraiseQL::Security::AuthorizeBuilder.create
        .rule("isAdmin($context)")
        .operations("create,delete")
        .description("Only applies to create and delete operations")
        .build

      expect(config.operations).to eq("create,delete")
    end

    it 'creates authorization with caching' do
      config = FraiseQL::Security::AuthorizeBuilder.create
        .rule("checkAuthorization($context)")
        .cacheable(true)
        .cache_duration_seconds(3600)
        .build

      expect(config.cacheable).to be true
      expect(config.cache_duration_seconds).to eq(3600)
    end

    it 'creates authorization without caching' do
      config = FraiseQL::Security::AuthorizeBuilder.create
        .rule("checkSensitiveAuthorization($context)")
        .cacheable(false)
        .build

      expect(config.cacheable).to be false
    end

    it 'creates multiple authorization rules' do
      config1 = FraiseQL::Security::AuthorizeBuilder.create
        .rule("isOwner($context.userId, $field.ownerId)")
        .description("Ownership check")
        .build

      config2 = FraiseQL::Security::AuthorizeBuilder.create
        .rule("hasScope($context, 'read:notes')")
        .description("Scope check")
        .build

      expect(config1.rule).not_to eq(config2.rule)
    end

    it 'supports fluent chaining' do
      config = FraiseQL::Security::AuthorizeBuilder.create
        .rule("isOwner($context.userId, $field.ownerId)")
        .description("Ownership authorization")
        .error_message("You can only access your own notes")
        .recursive(false)
        .operations("read,update")
        .cacheable(true)
        .cache_duration_seconds(600)
        .build

      expect(config.rule).to eq("isOwner($context.userId, $field.ownerId)")
      expect(config.description).to eq("Ownership authorization")
      expect(config.error_message).to eq("You can only access your own notes")
      expect(config.recursive).to be false
      expect(config.operations).to eq("read,update")
      expect(config.cacheable).to be true
      expect(config.cache_duration_seconds).to eq(600)
    end

    it 'supports include syntax' do
      class ProtectedNote
        include FraiseQL::Security::Authorize

        authorize rule: "isOwner($context.userId, $field.ownerId)",
                  description: "Ownership check"
      end

      expect(ProtectedNote.authorization_config[:rule]).to eq("isOwner($context.userId, $field.ownerId)")
    end

    it 'supports include with full configuration' do
      class FullyConfiguredNote
        include FraiseQL::Security::Authorize

        authorize rule: "isOwner($context.userId, $field.ownerId)",
                  description: "Ownership check",
                  error_message: "Access denied",
                  recursive: true,
                  operations: "read",
                  cacheable: false,
                  cache_duration_seconds: 0
      end

      config = FullyConfiguredNote.authorization_config
      expect(config[:rule]).to eq("isOwner($context.userId, $field.ownerId)")
      expect(config[:recursive]).to be true
    end
  end

  describe 'AuthorizeConfig' do
    it 'converts to hash' do
      config = FraiseQL::Security::AuthorizeConfig.new(
        rule: "test_rule",
        description: "Test",
        cacheable: true,
        cache_duration_seconds: 300
      )

      hash = config.to_h
      expect(hash[:rule]).to eq("test_rule")
      expect(hash[:description]).to eq("Test")
    end
  end
end
