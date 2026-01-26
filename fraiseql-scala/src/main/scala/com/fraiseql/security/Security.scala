package com.fraiseql.security

// Role matching strategies
sealed trait RoleMatchStrategy {
  def value: String
}
object RoleMatchStrategy {
  case object Any extends RoleMatchStrategy {
    def value = "any"
  }
  case object All extends RoleMatchStrategy {
    def value = "all"
  }
  case object Exactly extends RoleMatchStrategy {
    def value = "exactly"
  }

  def fromString(s: String): Option[RoleMatchStrategy] = s.toLowerCase match {
    case "any"     => Some(Any)
    case "all"     => Some(All)
    case "exactly" => Some(Exactly)
    case _         => None
  }
}

// Authorization policy types
sealed trait AuthzPolicyType {
  def value: String
}
object AuthzPolicyType {
  case object Rbac extends AuthzPolicyType {
    def value = "rbac"
  }
  case object Abac extends AuthzPolicyType {
    def value = "abac"
  }
  case object Custom extends AuthzPolicyType {
    def value = "custom"
  }
  case object Hybrid extends AuthzPolicyType {
    def value = "hybrid"
  }

  def fromString(s: String): Option[AuthzPolicyType] = s.toLowerCase match {
    case "rbac"   => Some(Rbac)
    case "abac"   => Some(Abac)
    case "custom" => Some(Custom)
    case "hybrid" => Some(Hybrid)
    case _        => None
  }
}

// Custom authorization configuration
case class AuthorizeConfig(
    rule: String = "",
    policy: String = "",
    description: String = "",
    errorMessage: String = "",
    recursive: Boolean = false,
    operations: String = "",
    cacheable: Boolean = true,
    cacheDurationSeconds: Int = 300
) {
  def toMap: Map[String, Any] = Map(
    "rule" -> rule,
    "policy" -> policy,
    "description" -> description,
    "errorMessage" -> errorMessage,
    "recursive" -> recursive,
    "operations" -> operations,
    "cacheable" -> cacheable,
    "cacheDurationSeconds" -> cacheDurationSeconds
  )
}

// Role-based access control configuration
case class RoleRequiredConfig(
    roles: List[String] = List(),
    strategy: RoleMatchStrategy = RoleMatchStrategy.Any,
    hierarchy: Boolean = false,
    description: String = "",
    errorMessage: String = "",
    operations: String = "",
    inherit: Boolean = false,
    cacheable: Boolean = true,
    cacheDurationSeconds: Int = 300
) {
  def toMap: Map[String, Any] = Map(
    "roles" -> roles,
    "strategy" -> strategy.value,
    "hierarchy" -> hierarchy,
    "description" -> description,
    "errorMessage" -> errorMessage,
    "operations" -> operations,
    "inherit" -> inherit,
    "cacheable" -> cacheable,
    "cacheDurationSeconds" -> cacheDurationSeconds
  )
}

// Authorization policy configuration
case class AuthzPolicyConfig(
    name: String,
    policyType: AuthzPolicyType = AuthzPolicyType.Custom,
    description: String = "",
    rule: String = "",
    attributes: List[String] = List(),
    cacheable: Boolean = true,
    cacheDurationSeconds: Int = 300,
    recursive: Boolean = false,
    operations: String = "",
    auditLogging: Boolean = false,
    errorMessage: String = ""
) {
  def toMap: Map[String, Any] = Map(
    "name" -> name,
    "type" -> policyType.value,
    "description" -> description,
    "rule" -> rule,
    "attributes" -> attributes,
    "cacheable" -> cacheable,
    "cacheDurationSeconds" -> cacheDurationSeconds,
    "recursive" -> recursive,
    "operations" -> operations,
    "auditLogging" -> auditLogging,
    "errorMessage" -> errorMessage
  )
}

// Builder for custom authorization
class AuthorizeBuilder {
  private var rule = ""
  private var policy = ""
  private var description = ""
  private var errorMessage = ""
  private var recursive = false
  private var operations = ""
  private var cacheable = true
  private var cacheDurationSeconds = 300

  def withRule(r: String): AuthorizeBuilder = {
    rule = r
    this
  }
  def withPolicy(p: String): AuthorizeBuilder = {
    policy = p
    this
  }
  def withDescription(d: String): AuthorizeBuilder = {
    description = d
    this
  }
  def withErrorMessage(e: String): AuthorizeBuilder = {
    errorMessage = e
    this
  }
  def withRecursive(r: Boolean): AuthorizeBuilder = {
    recursive = r
    this
  }
  def withOperations(o: String): AuthorizeBuilder = {
    operations = o
    this
  }
  def withCacheable(c: Boolean): AuthorizeBuilder = {
    cacheable = c
    this
  }
  def withCacheDurationSeconds(d: Int): AuthorizeBuilder = {
    cacheDurationSeconds = d
    this
  }
  def build(): AuthorizeConfig = AuthorizeConfig(
    rule, policy, description, errorMessage, recursive, operations, cacheable, cacheDurationSeconds
  )
}

// Builder for RBAC
class RoleRequiredBuilder {
  private var roles = List[String]()
  private var strategy = RoleMatchStrategy.Any
  private var hierarchy = false
  private var description = ""
  private var errorMessage = ""
  private var operations = ""
  private var inherit = false
  private var cacheable = true
  private var cacheDurationSeconds = 300

  def withRoles(r: List[String]): RoleRequiredBuilder = {
    roles = r
    this
  }
  def withStrategy(s: RoleMatchStrategy): RoleRequiredBuilder = {
    strategy = s
    this
  }
  def withHierarchy(h: Boolean): RoleRequiredBuilder = {
    hierarchy = h
    this
  }
  def withDescription(d: String): RoleRequiredBuilder = {
    description = d
    this
  }
  def withErrorMessage(e: String): RoleRequiredBuilder = {
    errorMessage = e
    this
  }
  def withOperations(o: String): RoleRequiredBuilder = {
    operations = o
    this
  }
  def withInherit(i: Boolean): RoleRequiredBuilder = {
    inherit = i
    this
  }
  def withCacheable(c: Boolean): RoleRequiredBuilder = {
    cacheable = c
    this
  }
  def withCacheDurationSeconds(d: Int): RoleRequiredBuilder = {
    cacheDurationSeconds = d
    this
  }
  def build(): RoleRequiredConfig = RoleRequiredConfig(
    roles, strategy, hierarchy, description, errorMessage, operations, inherit, cacheable,
    cacheDurationSeconds
  )
}

// Builder for authorization policies
class AuthzPolicyBuilder(val name: String) {
  private var policyType = AuthzPolicyType.Custom
  private var description = ""
  private var rule = ""
  private var attributes = List[String]()
  private var cacheable = true
  private var cacheDurationSeconds = 300
  private var recursive = false
  private var operations = ""
  private var auditLogging = false
  private var errorMessage = ""

  def withType(t: AuthzPolicyType): AuthzPolicyBuilder = {
    policyType = t
    this
  }
  def withDescription(d: String): AuthzPolicyBuilder = {
    description = d
    this
  }
  def withRule(r: String): AuthzPolicyBuilder = {
    rule = r
    this
  }
  def withAttributes(a: List[String]): AuthzPolicyBuilder = {
    attributes = a
    this
  }
  def withCacheable(c: Boolean): AuthzPolicyBuilder = {
    cacheable = c
    this
  }
  def withCacheDurationSeconds(d: Int): AuthzPolicyBuilder = {
    cacheDurationSeconds = d
    this
  }
  def withRecursive(r: Boolean): AuthzPolicyBuilder = {
    recursive = r
    this
  }
  def withOperations(o: String): AuthzPolicyBuilder = {
    operations = o
    this
  }
  def withAuditLogging(a: Boolean): AuthzPolicyBuilder = {
    auditLogging = a
    this
  }
  def withErrorMessage(e: String): AuthzPolicyBuilder = {
    errorMessage = e
    this
  }
  def build(): AuthzPolicyConfig = AuthzPolicyConfig(
    name, policyType, description, rule, attributes, cacheable, cacheDurationSeconds,
    recursive, operations, auditLogging, errorMessage
  )
}

object Authorization {
  def authorize() = new AuthorizeBuilder()
  def roleRequired() = new RoleRequiredBuilder()
  def policy(name: String) = new AuthzPolicyBuilder(name)
}
