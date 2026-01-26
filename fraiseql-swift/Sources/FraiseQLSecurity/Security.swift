/// FraiseQL Swift - Security module with 100% feature parity
///
/// Provides declarative, type-safe authorization and security configuration
/// across 11 authoring languages.

import Foundation

// MARK: - Enums

/// Role matching strategies for RBAC
public enum RoleMatchStrategy: String, Codable {
    /// At least one role must match
    case any
    /// All roles must match
    case all
    /// Exactly these roles
    case exactly
}

/// Authorization policy types
public enum AuthzPolicyType: String, Codable {
    /// Role-based access control
    case rbac
    /// Attribute-based access control
    case abac
    /// Custom authorization rules
    case custom
    /// Hybrid approach combining multiple models
    case hybrid
}

// MARK: - Configuration Structs

/// Configuration for custom authorization rules
public struct AuthorizeConfig: Codable {
    /// Authorization rule expression
    public let rule: String
    /// Named policy reference
    public let policy: String
    /// Configuration description
    public let description: String
    /// Custom error message on denial
    public let errorMessage: String
    /// Apply rule recursively to nested types
    public let recursive: Bool
    /// Operation-specific rules (e.g., "read,create,update,delete")
    public let operations: String
    /// Enable result caching
    public let cacheable: Bool
    /// Cache duration in seconds
    public let cacheDurationSeconds: Int

    public init(
        rule: String = "",
        policy: String = "",
        description: String = "",
        errorMessage: String = "",
        recursive: Bool = false,
        operations: String = "",
        cacheable: Bool = true,
        cacheDurationSeconds: Int = 300
    ) {
        self.rule = rule
        self.policy = policy
        self.description = description
        self.errorMessage = errorMessage
        self.recursive = recursive
        self.operations = operations
        self.cacheable = cacheable
        self.cacheDurationSeconds = cacheDurationSeconds
    }

    /// Convert to Dictionary for serialization
    public func toDictionary() -> [String: Any] {
        [
            "rule": rule,
            "policy": policy,
            "description": description,
            "errorMessage": errorMessage,
            "recursive": recursive,
            "operations": operations,
            "cacheable": cacheable,
            "cacheDurationSeconds": cacheDurationSeconds
        ]
    }
}

/// Configuration for role-based access control
public struct RoleRequiredConfig: Codable {
    /// Required roles
    public let roles: [String]
    /// Role matching strategy
    public let strategy: RoleMatchStrategy
    /// Support role hierarchy
    public let hierarchy: Bool
    /// Description
    public let description: String
    /// Custom error message
    public let errorMessage: String
    /// Operation-specific rules
    public let operations: String
    /// Inherit from parent
    public let inherit: Bool
    /// Enable caching
    public let cacheable: Bool
    /// Cache duration in seconds
    public let cacheDurationSeconds: Int

    public init(
        roles: [String] = [],
        strategy: RoleMatchStrategy = .any,
        hierarchy: Bool = false,
        description: String = "",
        errorMessage: String = "",
        operations: String = "",
        inherit: Bool = false,
        cacheable: Bool = true,
        cacheDurationSeconds: Int = 300
    ) {
        self.roles = roles
        self.strategy = strategy
        self.hierarchy = hierarchy
        self.description = description
        self.errorMessage = errorMessage
        self.operations = operations
        self.inherit = inherit
        self.cacheable = cacheable
        self.cacheDurationSeconds = cacheDurationSeconds
    }

    /// Convert to Dictionary for serialization
    public func toDictionary() -> [String: Any] {
        [
            "roles": roles,
            "strategy": strategy.rawValue,
            "hierarchy": hierarchy,
            "description": description,
            "errorMessage": errorMessage,
            "operations": operations,
            "inherit": inherit,
            "cacheable": cacheable,
            "cacheDurationSeconds": cacheDurationSeconds
        ]
    }
}

/// Configuration for reusable authorization policies
public struct AuthzPolicyConfig: Codable {
    /// Policy name
    public let name: String
    /// Policy type
    public let type: AuthzPolicyType
    /// Description
    public let description: String
    /// Authorization rule
    public let rule: String
    /// ABAC attributes
    public let attributes: [String]
    /// Enable caching
    public let cacheable: Bool
    /// Cache duration in seconds
    public let cacheDurationSeconds: Int
    /// Apply recursively to nested types
    public let recursive: Bool
    /// Operation-specific rules
    public let operations: String
    /// Enable audit logging
    public let auditLogging: Bool
    /// Custom error message
    public let errorMessage: String

    public init(
        name: String,
        type: AuthzPolicyType = .custom,
        description: String = "",
        rule: String = "",
        attributes: [String] = [],
        cacheable: Bool = true,
        cacheDurationSeconds: Int = 300,
        recursive: Bool = false,
        operations: String = "",
        auditLogging: Bool = false,
        errorMessage: String = ""
    ) {
        self.name = name
        self.type = type
        self.description = description
        self.rule = rule
        self.attributes = attributes
        self.cacheable = cacheable
        self.cacheDurationSeconds = cacheDurationSeconds
        self.recursive = recursive
        self.operations = operations
        self.auditLogging = auditLogging
        self.errorMessage = errorMessage
    }

    /// Convert to Dictionary for serialization
    public func toDictionary() -> [String: Any] {
        [
            "name": name,
            "type": type.rawValue,
            "description": description,
            "rule": rule,
            "attributes": attributes,
            "cacheable": cacheable,
            "cacheDurationSeconds": cacheDurationSeconds,
            "recursive": recursive,
            "operations": operations,
            "auditLogging": auditLogging,
            "errorMessage": errorMessage
        ]
    }
}

// MARK: - Builders

/// Fluent builder for custom authorization rules
public class AuthorizeBuilder {
    private var rule: String = ""
    private var policy: String = ""
    private var description: String = ""
    private var errorMessage: String = ""
    private var recursive: Bool = false
    private var operations: String = ""
    private var cacheable: Bool = true
    private var cacheDurationSeconds: Int = 300

    public init() {}

    @discardableResult
    public func rule(_ rule: String) -> Self {
        self.rule = rule
        return self
    }

    @discardableResult
    public func policy(_ policy: String) -> Self {
        self.policy = policy
        return self
    }

    @discardableResult
    public func description(_ description: String) -> Self {
        self.description = description
        return self
    }

    @discardableResult
    public func errorMessage(_ errorMessage: String) -> Self {
        self.errorMessage = errorMessage
        return self
    }

    @discardableResult
    public func recursive(_ recursive: Bool) -> Self {
        self.recursive = recursive
        return self
    }

    @discardableResult
    public func operations(_ operations: String) -> Self {
        self.operations = operations
        return self
    }

    @discardableResult
    public func cacheable(_ cacheable: Bool) -> Self {
        self.cacheable = cacheable
        return self
    }

    @discardableResult
    public func cacheDurationSeconds(_ duration: Int) -> Self {
        self.cacheDurationSeconds = duration
        return self
    }

    public func build() -> AuthorizeConfig {
        AuthorizeConfig(
            rule: rule,
            policy: policy,
            description: description,
            errorMessage: errorMessage,
            recursive: recursive,
            operations: operations,
            cacheable: cacheable,
            cacheDurationSeconds: cacheDurationSeconds
        )
    }
}

/// Fluent builder for role-based access control
public class RoleRequiredBuilder {
    private var roles: [String] = []
    private var strategy: RoleMatchStrategy = .any
    private var hierarchy: Bool = false
    private var description: String = ""
    private var errorMessage: String = ""
    private var operations: String = ""
    private var inherit: Bool = false
    private var cacheable: Bool = true
    private var cacheDurationSeconds: Int = 300

    public init() {}

    @discardableResult
    public func roles(_ roles: [String]) -> Self {
        self.roles = roles
        return self
    }

    @discardableResult
    public func strategy(_ strategy: RoleMatchStrategy) -> Self {
        self.strategy = strategy
        return self
    }

    @discardableResult
    public func hierarchy(_ hierarchy: Bool) -> Self {
        self.hierarchy = hierarchy
        return self
    }

    @discardableResult
    public func description(_ description: String) -> Self {
        self.description = description
        return self
    }

    @discardableResult
    public func errorMessage(_ errorMessage: String) -> Self {
        self.errorMessage = errorMessage
        return self
    }

    @discardableResult
    public func operations(_ operations: String) -> Self {
        self.operations = operations
        return self
    }

    @discardableResult
    public func inherit(_ inherit: Bool) -> Self {
        self.inherit = inherit
        return self
    }

    @discardableResult
    public func cacheable(_ cacheable: Bool) -> Self {
        self.cacheable = cacheable
        return self
    }

    @discardableResult
    public func cacheDurationSeconds(_ duration: Int) -> Self {
        self.cacheDurationSeconds = duration
        return self
    }

    public func build() -> RoleRequiredConfig {
        RoleRequiredConfig(
            roles: roles,
            strategy: strategy,
            hierarchy: hierarchy,
            description: description,
            errorMessage: errorMessage,
            operations: operations,
            inherit: inherit,
            cacheable: cacheable,
            cacheDurationSeconds: cacheDurationSeconds
        )
    }
}

/// Fluent builder for authorization policies
public class AuthzPolicyBuilder {
    private let name: String
    private var type: AuthzPolicyType = .custom
    private var description: String = ""
    private var rule: String = ""
    private var attributes: [String] = []
    private var cacheable: Bool = true
    private var cacheDurationSeconds: Int = 300
    private var recursive: Bool = false
    private var operations: String = ""
    private var auditLogging: Bool = false
    private var errorMessage: String = ""

    public init(_ name: String) {
        self.name = name
    }

    @discardableResult
    public func type(_ type: AuthzPolicyType) -> Self {
        self.type = type
        return self
    }

    @discardableResult
    public func description(_ description: String) -> Self {
        self.description = description
        return self
    }

    @discardableResult
    public func rule(_ rule: String) -> Self {
        self.rule = rule
        return self
    }

    @discardableResult
    public func attributes(_ attributes: [String]) -> Self {
        self.attributes = attributes
        return self
    }

    @discardableResult
    public func cacheable(_ cacheable: Bool) -> Self {
        self.cacheable = cacheable
        return self
    }

    @discardableResult
    public func cacheDurationSeconds(_ duration: Int) -> Self {
        self.cacheDurationSeconds = duration
        return self
    }

    @discardableResult
    public func recursive(_ recursive: Bool) -> Self {
        self.recursive = recursive
        return self
    }

    @discardableResult
    public func operations(_ operations: String) -> Self {
        self.operations = operations
        return self
    }

    @discardableResult
    public func auditLogging(_ auditLogging: Bool) -> Self {
        self.auditLogging = auditLogging
        return self
    }

    @discardableResult
    public func errorMessage(_ errorMessage: String) -> Self {
        self.errorMessage = errorMessage
        return self
    }

    public func build() -> AuthzPolicyConfig {
        AuthzPolicyConfig(
            name: name,
            type: type,
            description: description,
            rule: rule,
            attributes: attributes,
            cacheable: cacheable,
            cacheDurationSeconds: cacheDurationSeconds,
            recursive: recursive,
            operations: operations,
            auditLogging: auditLogging,
            errorMessage: errorMessage
        )
    }
}
