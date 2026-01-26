module FraiseQL
  module Security
    # Role matching strategies for RBAC
    module RoleMatchStrategy
      ANY = 'any'
      ALL = 'all'
      EXACTLY = 'exactly'
    end

    # Authorization policy types
    module AuthzPolicyType
      RBAC = 'rbac'
      ABAC = 'abac'
      CUSTOM = 'custom'
      HYBRID = 'hybrid'
    end

    # Configuration for custom authorization rules
    class AuthorizeConfig
      attr_accessor :rule, :policy, :description, :error_message, :recursive,
                    :operations, :cacheable, :cache_duration_seconds

      def initialize(
        rule: '',
        policy: '',
        description: '',
        error_message: '',
        recursive: false,
        operations: '',
        cacheable: true,
        cache_duration_seconds: 300
      )
        @rule = rule
        @policy = policy
        @description = description
        @error_message = error_message
        @recursive = recursive
        @operations = operations
        @cacheable = cacheable
        @cache_duration_seconds = cache_duration_seconds
      end

      def to_h
        {}.tap do |h|
          h[:rule] = @rule if @rule.to_s.length > 0
          h[:policy] = @policy if @policy.to_s.length > 0
          h[:description] = @description if @description.to_s.length > 0
          h[:error_message] = @error_message if @error_message.to_s.length > 0
          h[:recursive] = @recursive if @recursive
          h[:operations] = @operations if @operations.to_s.length > 0
          h[:cacheable] = @cacheable if @cacheable
          h[:cache_duration_seconds] = @cache_duration_seconds if @cacheable
        end
      end
    end

    # Configuration for role-based access control
    class RoleRequiredConfig
      attr_accessor :roles, :strategy, :hierarchy, :description, :error_message,
                    :operations, :inherit, :cacheable, :cache_duration_seconds

      def initialize(
        roles: [],
        strategy: RoleMatchStrategy::ANY,
        hierarchy: false,
        description: '',
        error_message: '',
        operations: '',
        inherit: true,
        cacheable: true,
        cache_duration_seconds: 600
      )
        @roles = roles
        @strategy = strategy
        @hierarchy = hierarchy
        @description = description
        @error_message = error_message
        @operations = operations
        @inherit = inherit
        @cacheable = cacheable
        @cache_duration_seconds = cache_duration_seconds
      end

      def to_h
        {}.tap do |h|
          h[:roles] = @roles if @roles.any?
          h[:strategy] = @strategy unless @strategy == RoleMatchStrategy::ANY
          h[:hierarchy] = @hierarchy if @hierarchy
          h[:description] = @description if @description.to_s.length > 0
          h[:error_message] = @error_message if @error_message.to_s.length > 0
          h[:operations] = @operations if @operations.to_s.length > 0
          h[:inherit] = @inherit unless @inherit
          h[:cacheable] = @cacheable if @cacheable
          h[:cache_duration_seconds] = @cache_duration_seconds if @cacheable
        end
      end
    end

    # Configuration for reusable authorization policies
    class AuthzPolicyConfig
      attr_accessor :name, :description, :rule, :attributes, :type, :cacheable,
                    :cache_duration_seconds, :recursive, :operations,
                    :audit_logging, :error_message

      def initialize(
        name:,
        description: '',
        rule: '',
        attributes: [],
        type: AuthzPolicyType::CUSTOM,
        cacheable: true,
        cache_duration_seconds: 300,
        recursive: false,
        operations: '',
        audit_logging: false,
        error_message: ''
      )
        @name = name
        @description = description
        @rule = rule
        @attributes = attributes
        @type = type
        @cacheable = cacheable
        @cache_duration_seconds = cache_duration_seconds
        @recursive = recursive
        @operations = operations
        @audit_logging = audit_logging
        @error_message = error_message
      end

      def to_h
        {
          name: @name
        }.tap do |h|
          h[:description] = @description if @description.to_s.length > 0
          h[:rule] = @rule if @rule.to_s.length > 0
          h[:attributes] = @attributes if @attributes.any?
          h[:type] = @type unless @type == AuthzPolicyType::CUSTOM
          h[:cacheable] = @cacheable if @cacheable
          h[:cache_duration_seconds] = @cache_duration_seconds if @cacheable
          h[:recursive] = @recursive if @recursive
          h[:operations] = @operations if @operations.to_s.length > 0
          h[:audit_logging] = @audit_logging if @audit_logging
          h[:error_message] = @error_message if @error_message.to_s.length > 0
        end
      end
    end

    # Builder for custom authorization rules
    #
    # Example:
    #   AuthorizeBuilder.create
    #     .rule("isOwner($context.userId, $field.ownerId)")
    #     .description("Ensures users can only access their own notes")
    #     .build
    class AuthorizeBuilder
      def initialize
        @config = AuthorizeConfig.new
      end

      def self.create
        new
      end

      def rule(rule_expr)
        @config.rule = rule_expr
        self
      end

      def policy(policy_name)
        @config.policy = policy_name
        self
      end

      def description(desc)
        @config.description = desc
        self
      end

      def error_message(msg)
        @config.error_message = msg
        self
      end

      def recursive(flag)
        @config.recursive = flag
        self
      end

      def operations(ops)
        @config.operations = ops
        self
      end

      def cacheable(flag)
        @config.cacheable = flag
        self
      end

      def cache_duration_seconds(duration)
        @config.cache_duration_seconds = duration
        self
      end

      def build
        AuthorizeConfig.new(
          rule: @config.rule,
          policy: @config.policy,
          description: @config.description,
          error_message: @config.error_message,
          recursive: @config.recursive,
          operations: @config.operations,
          cacheable: @config.cacheable,
          cache_duration_seconds: @config.cache_duration_seconds
        )
      end
    end

    # Builder for role-based access control rules
    #
    # Example:
    #   RoleRequiredBuilder.create
    #     .roles('manager', 'director')
    #     .strategy(RoleMatchStrategy::ANY)
    #     .description("Managers and directors can view salaries")
    #     .build
    class RoleRequiredBuilder
      def initialize
        @config = RoleRequiredConfig.new
      end

      def self.create
        new
      end

      def roles(*role_list)
        @config.roles = role_list
        self
      end

      def roles_array(role_list)
        @config.roles = role_list
        self
      end

      def strategy(strat)
        @config.strategy = strat
        self
      end

      def hierarchy(flag)
        @config.hierarchy = flag
        self
      end

      def description(desc)
        @config.description = desc
        self
      end

      def error_message(msg)
        @config.error_message = msg
        self
      end

      def operations(ops)
        @config.operations = ops
        self
      end

      def inherit(flag)
        @config.inherit = flag
        self
      end

      def cacheable(flag)
        @config.cacheable = flag
        self
      end

      def cache_duration_seconds(duration)
        @config.cache_duration_seconds = duration
        self
      end

      def build
        RoleRequiredConfig.new(
          roles: @config.roles,
          strategy: @config.strategy,
          hierarchy: @config.hierarchy,
          description: @config.description,
          error_message: @config.error_message,
          operations: @config.operations,
          inherit: @config.inherit,
          cacheable: @config.cacheable,
          cache_duration_seconds: @config.cache_duration_seconds
        )
      end
    end

    # Builder for reusable authorization policies
    #
    # Example:
    #   AuthzPolicyBuilder.create('piiAccess')
    #     .type(AuthzPolicyType::RBAC)
    #     .rule("hasRole($context, 'data_manager') OR hasScope($context, 'read:pii')")
    #     .description("Access to Personally Identifiable Information")
    #     .build
    class AuthzPolicyBuilder
      def initialize(name)
        @config = AuthzPolicyConfig.new(name: name)
      end

      def self.create(name)
        new(name)
      end

      def description(desc)
        @config.description = desc
        self
      end

      def rule(rule_expr)
        @config.rule = rule_expr
        self
      end

      def attributes(*attr_list)
        @config.attributes = attr_list
        self
      end

      def attributes_array(attr_list)
        @config.attributes = attr_list
        self
      end

      def type(policy_type)
        @config.type = policy_type
        self
      end

      def cacheable(flag)
        @config.cacheable = flag
        self
      end

      def cache_duration_seconds(duration)
        @config.cache_duration_seconds = duration
        self
      end

      def recursive(flag)
        @config.recursive = flag
        self
      end

      def operations(ops)
        @config.operations = ops
        self
      end

      def audit_logging(flag)
        @config.audit_logging = flag
        self
      end

      def error_message(msg)
        @config.error_message = msg
        self
      end

      def build
        AuthzPolicyConfig.new(
          name: @config.name,
          description: @config.description,
          rule: @config.rule,
          attributes: @config.attributes,
          type: @config.type,
          cacheable: @config.cacheable,
          cache_duration_seconds: @config.cache_duration_seconds,
          recursive: @config.recursive,
          operations: @config.operations,
          audit_logging: @config.audit_logging,
          error_message: @config.error_message
        )
      end
    end

    # Module for declaring authorization on a class
    #
    # Example:
    #   class ProtectedNote
    #     include Authorize
    #     authorize rule: "isOwner($context.userId, $field.ownerId)",
    #               description: "Ownership check"
    #   end
    module Authorize
      def self.included(base)
        base.extend ClassMethods
      end

      module ClassMethods
        def authorize(options = {})
          @authorization_config = options
        end

        def authorization_config
          @authorization_config || {}
        end
      end
    end

    # Module for declaring role requirements on a class
    module RoleRequired
      def self.included(base)
        base.extend ClassMethods
      end

      module ClassMethods
        def require_role(options = {})
          @role_config = options
        end

        def role_config
          @role_config || {}
        end
      end
    end

    # Module for declaring authorization policies on a class
    module AuthzPolicy
      def self.included(base)
        base.extend ClassMethods
      end

      module ClassMethods
        def authz_policy(options = {})
          @policy_config = options
        end

        def policy_config
          @policy_config || {}
        end
      end
    end
  end
end
