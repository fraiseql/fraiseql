package fraiseql

import "encoding/json"

// RoleMatchStrategy defines how to match multiple roles
type RoleMatchStrategy string

const (
	// RoleMatchAny - User must have at least one role
	RoleMatchAny RoleMatchStrategy = "any"
	// RoleMatchAll - User must have all roles
	RoleMatchAll RoleMatchStrategy = "all"
	// RoleMatchExactly - User must have exactly these roles
	RoleMatchExactly RoleMatchStrategy = "exactly"
)

// AuthzPolicyType defines the type of authorization policy
type AuthzPolicyType string

const (
	// AuthzRBAC - Role-based access control
	AuthzRBAC AuthzPolicyType = "rbac"
	// AuthzABAC - Attribute-based access control
	AuthzABAC AuthzPolicyType = "abac"
	// AuthzCustom - Custom rule expressions
	AuthzCustom AuthzPolicyType = "custom"
	// AuthzHybrid - Hybrid approach combining multiple methods
	AuthzHybrid AuthzPolicyType = "hybrid"
)

// AuthorizeConfig holds configuration for custom authorization rules
type AuthorizeConfig struct {
	Rule                  string `json:"rule,omitempty"`
	Policy                string `json:"policy,omitempty"`
	Description           string `json:"description,omitempty"`
	ErrorMessage          string `json:"error_message,omitempty"`
	Recursive             bool   `json:"recursive,omitempty"`
	Operations            string `json:"operations,omitempty"`
	Cacheable             bool   `json:"cacheable,omitempty"`
	CacheDurationSeconds  int    `json:"cache_duration_seconds,omitempty"`
}

// RoleRequiredConfig holds configuration for role-based access control
type RoleRequiredConfig struct {
	Roles                 []string          `json:"roles,omitempty"`
	Strategy              RoleMatchStrategy `json:"strategy,omitempty"`
	Hierarchy             bool              `json:"hierarchy,omitempty"`
	Description           string            `json:"description,omitempty"`
	ErrorMessage          string            `json:"error_message,omitempty"`
	Operations            string            `json:"operations,omitempty"`
	Inherit               bool              `json:"inherit,omitempty"`
	Cacheable             bool              `json:"cacheable,omitempty"`
	CacheDurationSeconds  int               `json:"cache_duration_seconds,omitempty"`
}

// AuthzPolicyConfig holds configuration for authorization policies
type AuthzPolicyConfig struct {
	Name                  string          `json:"name"`
	Description           string          `json:"description,omitempty"`
	Rule                  string          `json:"rule,omitempty"`
	Attributes            []string        `json:"attributes,omitempty"`
	Type                  AuthzPolicyType `json:"type,omitempty"`
	Cacheable             bool            `json:"cacheable,omitempty"`
	CacheDurationSeconds  int             `json:"cache_duration_seconds,omitempty"`
	Recursive             bool            `json:"recursive,omitempty"`
	Operations            string          `json:"operations,omitempty"`
	AuditLogging          bool            `json:"audit_logging,omitempty"`
	ErrorMessage          string          `json:"error_message,omitempty"`
}

// AuthorizeBuilder provides a fluent API for defining authorization rules
type AuthorizeBuilder struct {
	config AuthorizeConfig
}

// Authorize creates a new authorization rule builder
//
// Example:
//   Authorize().
//     Rule("isOwner($context.userId, $field.ownerId)").
//     Description("Ensures users can only access their own notes").
//     Register()
func Authorize() *AuthorizeBuilder {
	return &AuthorizeBuilder{
		config: AuthorizeConfig{
			Cacheable:            true,
			CacheDurationSeconds: 300,
		},
	}
}

// Rule sets the authorization rule expression
func (ab *AuthorizeBuilder) Rule(rule string) *AuthorizeBuilder {
	ab.config.Rule = rule
	return ab
}

// Policy sets the reference to a named authorization policy
func (ab *AuthorizeBuilder) Policy(policy string) *AuthorizeBuilder {
	ab.config.Policy = policy
	return ab
}

// Description sets the description of what this rule protects
func (ab *AuthorizeBuilder) Description(desc string) *AuthorizeBuilder {
	ab.config.Description = desc
	return ab
}

// ErrorMessage sets the custom error message
func (ab *AuthorizeBuilder) ErrorMessage(msg string) *AuthorizeBuilder {
	ab.config.ErrorMessage = msg
	return ab
}

// Recursive sets whether to apply rule hierarchically to child fields
func (ab *AuthorizeBuilder) Recursive(recursive bool) *AuthorizeBuilder {
	ab.config.Recursive = recursive
	return ab
}

// Operations sets which operations this rule applies to (read, create, update, delete)
func (ab *AuthorizeBuilder) Operations(ops string) *AuthorizeBuilder {
	ab.config.Operations = ops
	return ab
}

// Cacheable sets whether to cache authorization decisions
func (ab *AuthorizeBuilder) Cacheable(cacheable bool) *AuthorizeBuilder {
	ab.config.Cacheable = cacheable
	return ab
}

// CacheDurationSeconds sets the cache duration in seconds
func (ab *AuthorizeBuilder) CacheDurationSeconds(duration int) *AuthorizeBuilder {
	ab.config.CacheDurationSeconds = duration
	return ab
}

// Config returns the current authorization configuration
func (ab *AuthorizeBuilder) Config() AuthorizeConfig {
	return ab.config
}

// RoleRequiredBuilder provides a fluent API for role-based access control
type RoleRequiredBuilder struct {
	config RoleRequiredConfig
}

// RoleRequired creates a new role-based access control builder
//
// Example:
//   RoleRequired().
//     Roles("manager", "director").
//     Strategy(RoleMatchAny).
//     Description("Requires manager or director role").
//     Register()
func RoleRequired() *RoleRequiredBuilder {
	return &RoleRequiredBuilder{
		config: RoleRequiredConfig{
			Strategy:             RoleMatchAny,
			Inherit:              true,
			Cacheable:            true,
			CacheDurationSeconds: 600,
		},
	}
}

// Roles sets the required roles (variadic for convenience)
func (rb *RoleRequiredBuilder) Roles(roles ...string) *RoleRequiredBuilder {
	rb.config.Roles = roles
	return rb
}

// RolesSlice sets the required roles from a slice
func (rb *RoleRequiredBuilder) RolesSlice(roles []string) *RoleRequiredBuilder {
	rb.config.Roles = roles
	return rb
}

// Strategy sets the role matching strategy
func (rb *RoleRequiredBuilder) Strategy(strategy RoleMatchStrategy) *RoleRequiredBuilder {
	rb.config.Strategy = strategy
	return rb
}

// Hierarchy sets whether roles form a hierarchy
func (rb *RoleRequiredBuilder) Hierarchy(hierarchy bool) *RoleRequiredBuilder {
	rb.config.Hierarchy = hierarchy
	return rb
}

// Description sets the description
func (rb *RoleRequiredBuilder) Description(desc string) *RoleRequiredBuilder {
	rb.config.Description = desc
	return rb
}

// ErrorMessage sets the custom error message
func (rb *RoleRequiredBuilder) ErrorMessage(msg string) *RoleRequiredBuilder {
	rb.config.ErrorMessage = msg
	return rb
}

// Operations sets which operations this rule applies to
func (rb *RoleRequiredBuilder) Operations(ops string) *RoleRequiredBuilder {
	rb.config.Operations = ops
	return rb
}

// Inherit sets whether to inherit role requirements from parent types
func (rb *RoleRequiredBuilder) Inherit(inherit bool) *RoleRequiredBuilder {
	rb.config.Inherit = inherit
	return rb
}

// Cacheable sets whether to cache role validation results
func (rb *RoleRequiredBuilder) Cacheable(cacheable bool) *RoleRequiredBuilder {
	rb.config.Cacheable = cacheable
	return rb
}

// CacheDurationSeconds sets the cache duration in seconds
func (rb *RoleRequiredBuilder) CacheDurationSeconds(duration int) *RoleRequiredBuilder {
	rb.config.CacheDurationSeconds = duration
	return rb
}

// Config returns the current role configuration
func (rb *RoleRequiredBuilder) Config() RoleRequiredConfig {
	return rb.config
}

// AuthzPolicyBuilder provides a fluent API for defining reusable authorization policies
type AuthzPolicyBuilder struct {
	config AuthzPolicyConfig
}

// AuthzPolicy creates a new authorization policy builder
//
// Example:
//   AuthzPolicy("piiAccess").
//     Type(AuthzRBAC).
//     Rule("hasRole($context, 'data_manager') OR hasScope($context, 'read:pii')").
//     Description("Access to Personally Identifiable Information").
//     Register()
func AuthzPolicy(name string) *AuthzPolicyBuilder {
	return &AuthzPolicyBuilder{
		config: AuthzPolicyConfig{
			Name:                 name,
			Type:                 AuthzCustom,
			Cacheable:            true,
			CacheDurationSeconds: 300,
			AuditLogging:         true,
		},
	}
}

// Description sets the policy description
func (apb *AuthzPolicyBuilder) Description(desc string) *AuthzPolicyBuilder {
	apb.config.Description = desc
	return apb
}

// Rule sets the authorization rule expression
func (apb *AuthzPolicyBuilder) Rule(rule string) *AuthzPolicyBuilder {
	apb.config.Rule = rule
	return apb
}

// Attributes sets the attribute conditions for ABAC policies
func (apb *AuthzPolicyBuilder) Attributes(attrs ...string) *AuthzPolicyBuilder {
	apb.config.Attributes = attrs
	return apb
}

// AttributesSlice sets the attribute conditions from a slice
func (apb *AuthzPolicyBuilder) AttributesSlice(attrs []string) *AuthzPolicyBuilder {
	apb.config.Attributes = attrs
	return apb
}

// Type sets the policy type
func (apb *AuthzPolicyBuilder) Type(policyType AuthzPolicyType) *AuthzPolicyBuilder {
	apb.config.Type = policyType
	return apb
}

// Cacheable sets whether to cache authorization decisions
func (apb *AuthzPolicyBuilder) Cacheable(cacheable bool) *AuthzPolicyBuilder {
	apb.config.Cacheable = cacheable
	return apb
}

// CacheDurationSeconds sets the cache duration in seconds
func (apb *AuthzPolicyBuilder) CacheDurationSeconds(duration int) *AuthzPolicyBuilder {
	apb.config.CacheDurationSeconds = duration
	return apb
}

// Recursive sets whether to apply recursively to nested types
func (apb *AuthzPolicyBuilder) Recursive(recursive bool) *AuthzPolicyBuilder {
	apb.config.Recursive = recursive
	return apb
}

// Operations sets which operations this policy applies to
func (apb *AuthzPolicyBuilder) Operations(ops string) *AuthzPolicyBuilder {
	apb.config.Operations = ops
	return apb
}

// AuditLogging sets whether to log access decisions
func (apb *AuthzPolicyBuilder) AuditLogging(audit bool) *AuthzPolicyBuilder {
	apb.config.AuditLogging = audit
	return apb
}

// ErrorMessage sets the custom error message
func (apb *AuthzPolicyBuilder) ErrorMessage(msg string) *AuthzPolicyBuilder {
	apb.config.ErrorMessage = msg
	return apb
}

// Register adds this policy to the schema registry
func (apb *AuthzPolicyBuilder) Register() {
	GetRegistry().RegisterAuthzPolicy(apb.config)
}

// Config returns the current policy configuration
func (apb *AuthzPolicyBuilder) Config() AuthzPolicyConfig {
	return apb.config
}

// MarshalJSON implements json.Marshaler for AuthorizeConfig
func (ac AuthorizeConfig) MarshalJSON() ([]byte, error) {
	type Alias AuthorizeConfig
	return json.Marshal(struct {
		*Alias
	}{
		Alias: (*Alias)(&ac),
	})
}

// MarshalJSON implements json.Marshaler for RoleRequiredConfig
func (rc RoleRequiredConfig) MarshalJSON() ([]byte, error) {
	type Alias RoleRequiredConfig
	return json.Marshal(struct {
		*Alias
	}{
		Alias: (*Alias)(&rc),
	})
}

// MarshalJSON implements json.Marshaler for AuthzPolicyConfig
func (pc AuthzPolicyConfig) MarshalJSON() ([]byte, error) {
	type Alias AuthzPolicyConfig
	return json.Marshal(struct {
		*Alias
	}{
		Alias: (*Alias)(&pc),
	})
}
