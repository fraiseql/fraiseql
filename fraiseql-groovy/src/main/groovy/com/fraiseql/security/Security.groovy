package com.fraiseql.security

enum RoleMatchStrategy {
    ANY('any'),
    ALL('all'),
    EXACTLY('exactly')

    final String value
    RoleMatchStrategy(String value) { this.value = value }
}

enum AuthzPolicyType {
    RBAC('rbac'),
    ABAC('abac'),
    CUSTOM('custom'),
    HYBRID('hybrid')

    final String value
    AuthzPolicyType(String value) { this.value = value }
}

@Immutable(includes=['rule', 'policy', 'description', 'errorMessage', 'recursive', 'operations', 'cacheable', 'cacheDurationSeconds'])
class AuthorizeConfig {
    String rule = ''
    String policy = ''
    String description = ''
    String errorMessage = ''
    boolean recursive = false
    String operations = ''
    boolean cacheable = true
    int cacheDurationSeconds = 300

    Map toMap() {
        [rule: rule, policy: policy, description: description, errorMessage: errorMessage,
         recursive: recursive, operations: operations, cacheable: cacheable,
         cacheDurationSeconds: cacheDurationSeconds]
    }
}

@Immutable(includes=['roles', 'strategy', 'hierarchy', 'description', 'errorMessage', 'operations', 'inherit', 'cacheable', 'cacheDurationSeconds'])
class RoleRequiredConfig {
    List<String> roles = []
    RoleMatchStrategy strategy = RoleMatchStrategy.ANY
    boolean hierarchy = false
    String description = ''
    String errorMessage = ''
    String operations = ''
    boolean inherit = false
    boolean cacheable = true
    int cacheDurationSeconds = 300

    Map toMap() {
        [roles: roles, strategy: strategy.value, hierarchy: hierarchy, description: description,
         errorMessage: errorMessage, operations: operations, inherit: inherit,
         cacheable: cacheable, cacheDurationSeconds: cacheDurationSeconds]
    }
}

@Immutable(includes=['name', 'policyType', 'description', 'rule', 'attributes', 'cacheable', 'cacheDurationSeconds', 'recursive', 'operations', 'auditLogging', 'errorMessage'])
class AuthzPolicyConfig {
    String name
    AuthzPolicyType policyType = AuthzPolicyType.CUSTOM
    String description = ''
    String rule = ''
    List<String> attributes = []
    boolean cacheable = true
    int cacheDurationSeconds = 300
    boolean recursive = false
    String operations = ''
    boolean auditLogging = false
    String errorMessage = ''

    Map toMap() {
        [name: name, type: policyType.value, description: description, rule: rule,
         attributes: attributes, cacheable: cacheable, cacheDurationSeconds: cacheDurationSeconds,
         recursive: recursive, operations: operations, auditLogging: auditLogging,
         errorMessage: errorMessage]
    }
}

class AuthorizeBuilder {
    String rule = ''
    String policy = ''
    String description = ''
    String errorMessage = ''
    boolean recursive = false
    String operations = ''
    boolean cacheable = true
    int cacheDurationSeconds = 300

    AuthorizeBuilder rule(String r) { rule = r; this }
    AuthorizeBuilder policy(String p) { policy = p; this }
    AuthorizeBuilder description(String d) { description = d; this }
    AuthorizeBuilder errorMessage(String e) { errorMessage = e; this }
    AuthorizeBuilder recursive(boolean r) { recursive = r; this }
    AuthorizeBuilder operations(String o) { operations = o; this }
    AuthorizeBuilder cacheable(boolean c) { cacheable = c; this }
    AuthorizeBuilder cacheDurationSeconds(int d) { cacheDurationSeconds = d; this }

    AuthorizeConfig build() {
        new AuthorizeConfig(rule, policy, description, errorMessage, recursive, operations, cacheable, cacheDurationSeconds)
    }
}

class RoleRequiredBuilder {
    List<String> roles = []
    RoleMatchStrategy strategy = RoleMatchStrategy.ANY
    boolean hierarchy = false
    String description = ''
    String errorMessage = ''
    String operations = ''
    boolean inherit = false
    boolean cacheable = true
    int cacheDurationSeconds = 300

    RoleRequiredBuilder roles(List<String> r) { roles = r; this }
    RoleRequiredBuilder strategy(RoleMatchStrategy s) { strategy = s; this }
    RoleRequiredBuilder hierarchy(boolean h) { hierarchy = h; this }
    RoleRequiredBuilder description(String d) { description = d; this }
    RoleRequiredBuilder errorMessage(String e) { errorMessage = e; this }
    RoleRequiredBuilder operations(String o) { operations = o; this }
    RoleRequiredBuilder inherit(boolean i) { inherit = i; this }
    RoleRequiredBuilder cacheable(boolean c) { cacheable = c; this }
    RoleRequiredBuilder cacheDurationSeconds(int d) { cacheDurationSeconds = d; this }

    RoleRequiredConfig build() {
        new RoleRequiredConfig(roles, strategy, hierarchy, description, errorMessage, operations, inherit, cacheable, cacheDurationSeconds)
    }
}

class AuthzPolicyBuilder {
    String name
    AuthzPolicyType policyType = AuthzPolicyType.CUSTOM
    String description = ''
    String rule = ''
    List<String> attributes = []
    boolean cacheable = true
    int cacheDurationSeconds = 300
    boolean recursive = false
    String operations = ''
    boolean auditLogging = false
    String errorMessage = ''

    AuthzPolicyBuilder(String n) { name = n }

    AuthzPolicyBuilder type(AuthzPolicyType t) { policyType = t; this }
    AuthzPolicyBuilder description(String d) { description = d; this }
    AuthzPolicyBuilder rule(String r) { rule = r; this }
    AuthzPolicyBuilder attributes(List<String> a) { attributes = a; this }
    AuthzPolicyBuilder cacheable(boolean c) { cacheable = c; this }
    AuthzPolicyBuilder cacheDurationSeconds(int d) { cacheDurationSeconds = d; this }
    AuthzPolicyBuilder recursive(boolean r) { recursive = r; this }
    AuthzPolicyBuilder operations(String o) { operations = o; this }
    AuthzPolicyBuilder auditLogging(boolean a) { auditLogging = a; this }
    AuthzPolicyBuilder errorMessage(String e) { errorMessage = e; this }

    AuthzPolicyConfig build() {
        new AuthzPolicyConfig(name, policyType, description, rule, attributes, cacheable, cacheDurationSeconds, recursive, operations, auditLogging, errorMessage)
    }
}
