using System;
using System.Collections.Generic;
using System.Linq;

namespace FraiseQL.Security
{
    /// <summary>
    /// Role matching strategies for RBAC
    /// </summary>
    public abstract record RoleMatchStrategy
    {
        public string Value { get; }

        private RoleMatchStrategy(string value)
        {
            Value = value;
        }

        /// <summary>At least one role must match</summary>
        public sealed record Any : RoleMatchStrategy
        {
            public Any() : base("any") { }
        }

        /// <summary>All roles must match</summary>
        public sealed record All : RoleMatchStrategy
        {
            public All() : base("all") { }
        }

        /// <summary>Exactly these roles</summary>
        public sealed record Exactly : RoleMatchStrategy
        {
            public Exactly() : base("exactly") { }
        }

        public static RoleMatchStrategy FromString(string value) =>
            value.ToLowerInvariant() switch
            {
                "any" => new Any(),
                "all" => new All(),
                "exactly" => new Exactly(),
                _ => throw new ArgumentException($"Unknown strategy: {value}")
            };
    }

    /// <summary>
    /// Authorization policy types
    /// </summary>
    public abstract record AuthzPolicyType
    {
        public string Value { get; }

        private AuthzPolicyType(string value)
        {
            Value = value;
        }

        /// <summary>Role-based access control</summary>
        public sealed record Rbac : AuthzPolicyType
        {
            public Rbac() : base("rbac") { }
        }

        /// <summary>Attribute-based access control</summary>
        public sealed record Abac : AuthzPolicyType
        {
            public Abac() : base("abac") { }
        }

        /// <summary>Custom authorization rules</summary>
        public sealed record Custom : AuthzPolicyType
        {
            public Custom() : base("custom") { }
        }

        /// <summary>Hybrid approach combining RBAC and ABAC</summary>
        public sealed record Hybrid : AuthzPolicyType
        {
            public Hybrid() : base("hybrid") { }
        }

        public static AuthzPolicyType FromString(string value) =>
            value.ToLowerInvariant() switch
            {
                "rbac" => new Rbac(),
                "abac" => new Abac(),
                "custom" => new Custom(),
                "hybrid" => new Hybrid(),
                _ => throw new ArgumentException($"Unknown policy type: {value}")
            };
    }

    /// <summary>
    /// Configuration for custom authorization rules
    /// </summary>
    public record AuthorizeConfig(
        string Rule = "",
        string Policy = "",
        string Description = "",
        string ErrorMessage = "",
        bool Recursive = false,
        string Operations = "",
        bool Cacheable = true,
        int CacheDurationSeconds = 300
    )
    {
        public Dictionary<string, object> ToDict() => new()
        {
            { "rule", Rule },
            { "policy", Policy },
            { "description", Description },
            { "errorMessage", ErrorMessage },
            { "recursive", Recursive },
            { "operations", Operations },
            { "cacheable", Cacheable },
            { "cacheDurationSeconds", CacheDurationSeconds }
        };
    }

    /// <summary>
    /// Configuration for role-based access control
    /// </summary>
    public record RoleRequiredConfig(
        List<string> Roles = null!,
        RoleMatchStrategy Strategy = null!,
        bool Hierarchy = false,
        string Description = "",
        string ErrorMessage = "",
        string Operations = "",
        bool Inherit = false,
        bool Cacheable = true,
        int CacheDurationSeconds = 300
    )
    {
        public RoleRequiredConfig() : this(
            new List<string>(),
            new RoleMatchStrategy.Any(),
            false,
            "",
            "",
            "",
            false,
            true,
            300
        ) { }

        public Dictionary<string, object> ToDict() => new()
        {
            { "roles", Roles },
            { "strategy", Strategy.Value },
            { "hierarchy", Hierarchy },
            { "description", Description },
            { "errorMessage", ErrorMessage },
            { "operations", Operations },
            { "inherit", Inherit },
            { "cacheable", Cacheable },
            { "cacheDurationSeconds", CacheDurationSeconds }
        };
    }

    /// <summary>
    /// Configuration for reusable authorization policies
    /// </summary>
    public record AuthzPolicyConfig(
        string Name,
        AuthzPolicyType Type = null!,
        string Description = "",
        string Rule = "",
        List<string> Attributes = null!,
        bool Cacheable = true,
        int CacheDurationSeconds = 300,
        bool Recursive = false,
        string Operations = "",
        bool AuditLogging = false,
        string ErrorMessage = ""
    )
    {
        public AuthzPolicyConfig(string name) : this(
            name,
            new AuthzPolicyType.Custom(),
            "",
            "",
            new List<string>(),
            true,
            300,
            false,
            "",
            false,
            ""
        ) { }

        public Dictionary<string, object> ToDict() => new()
        {
            { "name", Name },
            { "type", Type.Value },
            { "description", Description },
            { "rule", Rule },
            { "attributes", Attributes },
            { "cacheable", Cacheable },
            { "cacheDurationSeconds", CacheDurationSeconds },
            { "recursive", Recursive },
            { "operations", Operations },
            { "auditLogging", AuditLogging },
            { "errorMessage", ErrorMessage }
        };
    }

    /// <summary>
    /// Fluent builder for custom authorization rules
    /// </summary>
    public class AuthorizeBuilder
    {
        private string _rule = "";
        private string _policy = "";
        private string _description = "";
        private string _errorMessage = "";
        private bool _recursive = false;
        private string _operations = "";
        private bool _cacheable = true;
        private int _cacheDurationSeconds = 300;

        public AuthorizeBuilder Rule(string rule)
        {
            _rule = rule;
            return this;
        }

        public AuthorizeBuilder Policy(string policy)
        {
            _policy = policy;
            return this;
        }

        public AuthorizeBuilder Description(string description)
        {
            _description = description;
            return this;
        }

        public AuthorizeBuilder ErrorMessage(string errorMessage)
        {
            _errorMessage = errorMessage;
            return this;
        }

        public AuthorizeBuilder Recursive(bool recursive)
        {
            _recursive = recursive;
            return this;
        }

        public AuthorizeBuilder Operations(string operations)
        {
            _operations = operations;
            return this;
        }

        public AuthorizeBuilder Cacheable(bool cacheable)
        {
            _cacheable = cacheable;
            return this;
        }

        public AuthorizeBuilder CacheDurationSeconds(int duration)
        {
            _cacheDurationSeconds = duration;
            return this;
        }

        public AuthorizeConfig Build() =>
            new(
                _rule,
                _policy,
                _description,
                _errorMessage,
                _recursive,
                _operations,
                _cacheable,
                _cacheDurationSeconds
            );
    }

    /// <summary>
    /// Fluent builder for role-based access control
    /// </summary>
    public class RoleRequiredBuilder
    {
        private List<string> _roles = new();
        private RoleMatchStrategy _strategy = new RoleMatchStrategy.Any();
        private bool _hierarchy = false;
        private string _description = "";
        private string _errorMessage = "";
        private string _operations = "";
        private bool _inherit = false;
        private bool _cacheable = true;
        private int _cacheDurationSeconds = 300;

        public RoleRequiredBuilder Roles(params string[] roles)
        {
            _roles = roles.ToList();
            return this;
        }

        public RoleRequiredBuilder RolesArray(List<string> roles)
        {
            _roles = roles;
            return this;
        }

        public RoleRequiredBuilder Strategy(RoleMatchStrategy strategy)
        {
            _strategy = strategy;
            return this;
        }

        public RoleRequiredBuilder Hierarchy(bool hierarchy)
        {
            _hierarchy = hierarchy;
            return this;
        }

        public RoleRequiredBuilder Description(string description)
        {
            _description = description;
            return this;
        }

        public RoleRequiredBuilder ErrorMessage(string errorMessage)
        {
            _errorMessage = errorMessage;
            return this;
        }

        public RoleRequiredBuilder Operations(string operations)
        {
            _operations = operations;
            return this;
        }

        public RoleRequiredBuilder Inherit(bool inherit)
        {
            _inherit = inherit;
            return this;
        }

        public RoleRequiredBuilder Cacheable(bool cacheable)
        {
            _cacheable = cacheable;
            return this;
        }

        public RoleRequiredBuilder CacheDurationSeconds(int duration)
        {
            _cacheDurationSeconds = duration;
            return this;
        }

        public RoleRequiredConfig Build() =>
            new(
                _roles,
                _strategy,
                _hierarchy,
                _description,
                _errorMessage,
                _operations,
                _inherit,
                _cacheable,
                _cacheDurationSeconds
            );
    }

    /// <summary>
    /// Fluent builder for authorization policies
    /// </summary>
    public class AuthzPolicyBuilder
    {
        private readonly string _name;
        private AuthzPolicyType _type = new AuthzPolicyType.Custom();
        private string _description = "";
        private string _rule = "";
        private List<string> _attributes = new();
        private bool _cacheable = true;
        private int _cacheDurationSeconds = 300;
        private bool _recursive = false;
        private string _operations = "";
        private bool _auditLogging = false;
        private string _errorMessage = "";

        public AuthzPolicyBuilder(string name)
        {
            _name = name;
        }

        public AuthzPolicyBuilder Type(AuthzPolicyType type)
        {
            _type = type;
            return this;
        }

        public AuthzPolicyBuilder Description(string description)
        {
            _description = description;
            return this;
        }

        public AuthzPolicyBuilder Rule(string rule)
        {
            _rule = rule;
            return this;
        }

        public AuthzPolicyBuilder Attributes(params string[] attributes)
        {
            _attributes = attributes.ToList();
            return this;
        }

        public AuthzPolicyBuilder AttributesArray(List<string> attributes)
        {
            _attributes = attributes;
            return this;
        }

        public AuthzPolicyBuilder Cacheable(bool cacheable)
        {
            _cacheable = cacheable;
            return this;
        }

        public AuthzPolicyBuilder CacheDurationSeconds(int duration)
        {
            _cacheDurationSeconds = duration;
            return this;
        }

        public AuthzPolicyBuilder Recursive(bool recursive)
        {
            _recursive = recursive;
            return this;
        }

        public AuthzPolicyBuilder Operations(string operations)
        {
            _operations = operations;
            return this;
        }

        public AuthzPolicyBuilder AuditLogging(bool auditLogging)
        {
            _auditLogging = auditLogging;
            return this;
        }

        public AuthzPolicyBuilder ErrorMessage(string errorMessage)
        {
            _errorMessage = errorMessage;
            return this;
        }

        public AuthzPolicyConfig Build() =>
            new(
                _name,
                _type,
                _description,
                _rule,
                _attributes,
                _cacheable,
                _cacheDurationSeconds,
                _recursive,
                _operations,
                _auditLogging,
                _errorMessage
            );
    }

    /// <summary>
    /// Attribute for custom authorization rules
    /// </summary>
    [AttributeUsage(AttributeTargets.Class | AttributeTargets.Property | AttributeTargets.Method)]
    public class AuthorizeAttribute : Attribute
    {
        public string Rule { get; set; } = "";
        public string Policy { get; set; } = "";
        public string Description { get; set; } = "";
        public string ErrorMessage { get; set; } = "";
        public bool Recursive { get; set; } = false;
        public string Operations { get; set; } = "";
        public bool Cacheable { get; set; } = true;
        public int CacheDurationSeconds { get; set; } = 300;
    }

    /// <summary>
    /// Attribute for role-based access control
    /// </summary>
    [AttributeUsage(AttributeTargets.Class | AttributeTargets.Property | AttributeTargets.Method)]
    public class RoleRequiredAttribute : Attribute
    {
        public string[] Roles { get; set; } = Array.Empty<string>();
        public string Strategy { get; set; } = "any";
        public bool Hierarchy { get; set; } = false;
        public string Description { get; set; } = "";
        public string ErrorMessage { get; set; } = "";
        public string Operations { get; set; } = "";
        public bool Inherit { get; set; } = false;
        public bool Cacheable { get; set; } = true;
        public int CacheDurationSeconds { get; set; } = 300;
    }

    /// <summary>
    /// Attribute for authorization policies
    /// </summary>
    [AttributeUsage(AttributeTargets.Class | AttributeTargets.Property | AttributeTargets.Method)]
    public class AuthzPolicyAttribute : Attribute
    {
        public string Name { get; set; } = "";
        public string Type { get; set; } = "custom";
        public string Description { get; set; } = "";
        public string Rule { get; set; } = "";
        public string[] Attributes { get; set; } = Array.Empty<string>();
        public bool Cacheable { get; set; } = true;
        public int CacheDurationSeconds { get; set; } = 300;
        public bool Recursive { get; set; } = false;
        public string Operations { get; set; } = "";
        public bool AuditLogging { get; set; } = false;
        public string ErrorMessage { get; set; } = "";
    }
}
