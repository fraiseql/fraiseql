package com.fraiseql.security

/**
 * Sealed class for role matching strategies in RBAC
 */
sealed class RoleMatchStrategy(val value: String) {
    object Any : RoleMatchStrategy("any")
    object All : RoleMatchStrategy("all")
    object Exactly : RoleMatchStrategy("exactly")
}

/**
 * Sealed class for authorization policy types
 */
sealed class AuthzPolicyType(val value: String) {
    object Rbac : AuthzPolicyType("rbac")
    object Abac : AuthzPolicyType("abac")
    object Custom : AuthzPolicyType("custom")
    object Hybrid : AuthzPolicyType("hybrid")
}

/**
 * Configuration for custom authorization rules
 */
data class AuthorizeConfig(
    val rule: String = "",
    val policy: String = "",
    val description: String = "",
    val errorMessage: String = "",
    val recursive: Boolean = false,
    val operations: String = "",
    val cacheable: Boolean = true,
    val cacheDurationSeconds: Int = 300
) {
    fun toMap(): Map<String, Any> {
        val map = mutableMapOf<String, Any>()
        if (rule.isNotEmpty()) map["rule"] = rule
        if (policy.isNotEmpty()) map["policy"] = policy
        if (description.isNotEmpty()) map["description"] = description
        if (errorMessage.isNotEmpty()) map["error_message"] = errorMessage
        if (recursive) map["recursive"] = recursive
        if (operations.isNotEmpty()) map["operations"] = operations
        if (cacheable) {
            map["cacheable"] = cacheable
            map["cache_duration_seconds"] = cacheDurationSeconds
        }
        return map
    }
}

/**
 * Configuration for role-based access control
 */
data class RoleRequiredConfig(
    val roles: List<String> = emptyList(),
    val strategy: RoleMatchStrategy = RoleMatchStrategy.Any,
    val hierarchy: Boolean = false,
    val description: String = "",
    val errorMessage: String = "",
    val operations: String = "",
    val inherit: Boolean = true,
    val cacheable: Boolean = true,
    val cacheDurationSeconds: Int = 600
) {
    fun toMap(): Map<String, Any> {
        val map = mutableMapOf<String, Any>()
        if (roles.isNotEmpty()) map["roles"] = roles
        if (strategy != RoleMatchStrategy.Any) map["strategy"] = strategy.value
        if (hierarchy) map["hierarchy"] = hierarchy
        if (description.isNotEmpty()) map["description"] = description
        if (errorMessage.isNotEmpty()) map["error_message"] = errorMessage
        if (operations.isNotEmpty()) map["operations"] = operations
        if (!inherit) map["inherit"] = inherit
        if (cacheable) {
            map["cacheable"] = cacheable
            map["cache_duration_seconds"] = cacheDurationSeconds
        }
        return map
    }
}

/**
 * Configuration for reusable authorization policies
 */
data class AuthzPolicyConfig(
    val name: String,
    val description: String = "",
    val rule: String = "",
    val attributes: List<String> = emptyList(),
    val type: AuthzPolicyType = AuthzPolicyType.Custom,
    val cacheable: Boolean = true,
    val cacheDurationSeconds: Int = 300,
    val recursive: Boolean = false,
    val operations: String = "",
    val auditLogging: Boolean = false,
    val errorMessage: String = ""
) {
    fun toMap(): Map<String, Any> {
        val map = mutableMapOf<String, Any>("name" to name)
        if (description.isNotEmpty()) map["description"] = description
        if (rule.isNotEmpty()) map["rule"] = rule
        if (attributes.isNotEmpty()) map["attributes"] = attributes
        if (type != AuthzPolicyType.Custom) map["type"] = type.value
        if (cacheable) {
            map["cacheable"] = cacheable
            map["cache_duration_seconds"] = cacheDurationSeconds
        }
        if (recursive) map["recursive"] = recursive
        if (operations.isNotEmpty()) map["operations"] = operations
        if (auditLogging) map["audit_logging"] = auditLogging
        if (errorMessage.isNotEmpty()) map["error_message"] = errorMessage
        return map
    }
}

/**
 * Builder for custom authorization rules
 *
 * Example:
 * ```kotlin
 * AuthorizeBuilder()
 *   .rule("isOwner(\$context.userId, \$field.ownerId)")
 *   .description("Ensures users can only access their own notes")
 *   .build()
 * ```
 */
class AuthorizeBuilder {
    private var rule: String = ""
    private var policy: String = ""
    private var description: String = ""
    private var errorMessage: String = ""
    private var recursive: Boolean = false
    private var operations: String = ""
    private var cacheable: Boolean = true
    private var cacheDurationSeconds: Int = 300

    fun rule(rule: String) = apply { this.rule = rule }
    fun policy(policy: String) = apply { this.policy = policy }
    fun description(description: String) = apply { this.description = description }
    fun errorMessage(errorMessage: String) = apply { this.errorMessage = errorMessage }
    fun recursive(recursive: Boolean) = apply { this.recursive = recursive }
    fun operations(operations: String) = apply { this.operations = operations }
    fun cacheable(cacheable: Boolean) = apply { this.cacheable = cacheable }
    fun cacheDurationSeconds(duration: Int) = apply { this.cacheDurationSeconds = duration }

    fun build() = AuthorizeConfig(
        rule = rule,
        policy = policy,
        description = description,
        errorMessage = errorMessage,
        recursive = recursive,
        operations = operations,
        cacheable = cacheable,
        cacheDurationSeconds = cacheDurationSeconds
    )
}

/**
 * Builder for role-based access control rules
 *
 * Example:
 * ```kotlin
 * RoleRequiredBuilder()
 *   .roles("manager", "director")
 *   .strategy(RoleMatchStrategy.Any)
 *   .description("Managers and directors can view salaries")
 *   .build()
 * ```
 */
class RoleRequiredBuilder {
    private var roles: List<String> = emptyList()
    private var strategy: RoleMatchStrategy = RoleMatchStrategy.Any
    private var hierarchy: Boolean = false
    private var description: String = ""
    private var errorMessage: String = ""
    private var operations: String = ""
    private var inherit: Boolean = true
    private var cacheable: Boolean = true
    private var cacheDurationSeconds: Int = 600

    fun roles(vararg roles: String) = apply { this.roles = roles.toList() }
    fun rolesArray(roles: List<String>) = apply { this.roles = roles }
    fun strategy(strategy: RoleMatchStrategy) = apply { this.strategy = strategy }
    fun hierarchy(hierarchy: Boolean) = apply { this.hierarchy = hierarchy }
    fun description(description: String) = apply { this.description = description }
    fun errorMessage(errorMessage: String) = apply { this.errorMessage = errorMessage }
    fun operations(operations: String) = apply { this.operations = operations }
    fun inherit(inherit: Boolean) = apply { this.inherit = inherit }
    fun cacheable(cacheable: Boolean) = apply { this.cacheable = cacheable }
    fun cacheDurationSeconds(duration: Int) = apply { this.cacheDurationSeconds = duration }

    fun build() = RoleRequiredConfig(
        roles = roles,
        strategy = strategy,
        hierarchy = hierarchy,
        description = description,
        errorMessage = errorMessage,
        operations = operations,
        inherit = inherit,
        cacheable = cacheable,
        cacheDurationSeconds = cacheDurationSeconds
    )
}

/**
 * Builder for reusable authorization policies
 *
 * Example:
 * ```kotlin
 * AuthzPolicyBuilder("piiAccess")
 *   .type(AuthzPolicyType.Rbac)
 *   .rule("hasRole(\$context, 'data_manager') OR hasScope(\$context, 'read:pii')")
 *   .description("Access to Personally Identifiable Information")
 *   .build()
 * ```
 */
class AuthzPolicyBuilder(private val name: String) {
    private var description: String = ""
    private var rule: String = ""
    private var attributes: List<String> = emptyList()
    private var type: AuthzPolicyType = AuthzPolicyType.Custom
    private var cacheable: Boolean = true
    private var cacheDurationSeconds: Int = 300
    private var recursive: Boolean = false
    private var operations: String = ""
    private var auditLogging: Boolean = false
    private var errorMessage: String = ""

    fun description(description: String) = apply { this.description = description }
    fun rule(rule: String) = apply { this.rule = rule }
    fun attributes(vararg attributes: String) = apply { this.attributes = attributes.toList() }
    fun attributesArray(attributes: List<String>) = apply { this.attributes = attributes }
    fun type(type: AuthzPolicyType) = apply { this.type = type }
    fun cacheable(cacheable: Boolean) = apply { this.cacheable = cacheable }
    fun cacheDurationSeconds(duration: Int) = apply { this.cacheDurationSeconds = duration }
    fun recursive(recursive: Boolean) = apply { this.recursive = recursive }
    fun operations(operations: String) = apply { this.operations = operations }
    fun auditLogging(auditLogging: Boolean) = apply { this.auditLogging = auditLogging }
    fun errorMessage(errorMessage: String) = apply { this.errorMessage = errorMessage }

    fun build() = AuthzPolicyConfig(
        name = name,
        description = description,
        rule = rule,
        attributes = attributes,
        type = type,
        cacheable = cacheable,
        cacheDurationSeconds = cacheDurationSeconds,
        recursive = recursive,
        operations = operations,
        auditLogging = auditLogging,
        errorMessage = errorMessage
    )
}

/**
 * Annotation for custom authorization rules
 */
@Target(AnnotationTarget.CLASS, AnnotationTarget.PROPERTY)
@Retention(AnnotationRetention.RUNTIME)
annotation class Authorize(
    val rule: String = "",
    val policy: String = "",
    val description: String = "",
    val errorMessage: String = "",
    val recursive: Boolean = false,
    val operations: String = "",
    val cacheable: Boolean = true,
    val cacheDurationSeconds: Int = 300
)

/**
 * Annotation for role-based access control
 */
@Target(AnnotationTarget.CLASS, AnnotationTarget.PROPERTY)
@Retention(AnnotationRetention.RUNTIME)
annotation class RoleRequired(
    val roles: Array<String> = [],
    val strategy: String = "any",
    val hierarchy: Boolean = false,
    val description: String = "",
    val errorMessage: String = "",
    val operations: String = "",
    val inherit: Boolean = true,
    val cacheable: Boolean = true,
    val cacheDurationSeconds: Int = 600
)

/**
 * Annotation for authorization policies
 */
@Target(AnnotationTarget.CLASS, AnnotationTarget.PROPERTY)
@Retention(AnnotationRetention.RUNTIME)
annotation class AuthzPolicy(
    val name: String,
    val description: String = "",
    val rule: String = "",
    val attributes: Array<String> = [],
    val type: String = "custom",
    val cacheable: Boolean = true,
    val cacheDurationSeconds: Int = 300,
    val recursive: Boolean = false,
    val operations: String = "",
    val auditLogging: Boolean = false,
    val errorMessage: String = ""
)
