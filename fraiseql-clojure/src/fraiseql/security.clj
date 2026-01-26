(ns fraiseql.security
  "FraiseQL Clojure - Security module with 100% feature parity

   Provides declarative, type-safe authorization and security configuration
   across 14 authoring languages.")

;; Role matching strategies
(def ^:const ROLE_MATCH_ANY :any)
(def ^:const ROLE_MATCH_ALL :all)
(def ^:const ROLE_MATCH_EXACTLY :exactly)

(defn role-match-strategy? [v]
  (#{ROLE_MATCH_ANY ROLE_MATCH_ALL ROLE_MATCH_EXACTLY} v))

;; Authorization policy types
(def ^:const AUTHZ_POLICY_RBAC :rbac)
(def ^:const AUTHZ_POLICY_ABAC :abac)
(def ^:const AUTHZ_POLICY_CUSTOM :custom)
(def ^:const AUTHZ_POLICY_HYBRID :hybrid)

(defn authz-policy-type? [v]
  (#{AUTHZ_POLICY_RBAC AUTHZ_POLICY_ABAC AUTHZ_POLICY_CUSTOM AUTHZ_POLICY_HYBRID} v))

;; Default configurations
(defn authorize-config
  "Create a custom authorization configuration

   Options:
   - :rule          authorization rule expression
   - :policy        named policy reference
   - :description   configuration description
   - :error-message custom error message
   - :recursive     apply recursively (default false)
   - :operations    operation-specific rules
   - :cacheable     enable caching (default true)
   - :cache-duration cache duration in seconds (default 300)"
  [& {:keys [rule policy description error-message recursive operations
             cacheable cache-duration]
      :or {rule "" policy "" description "" error-message ""
           recursive false operations "" cacheable true cache-duration 300}}]
  {:rule rule
   :policy policy
   :description description
   :error-message error-message
   :recursive recursive
   :operations operations
   :cacheable cacheable
   :cache-duration-seconds cache-duration})

(defn role-required-config
  "Create a role-based access control configuration

   Options:
   - :roles            required roles (default [])
   - :strategy         matching strategy (any/all/exactly, default :any)
   - :hierarchy        support role hierarchy (default false)
   - :description      description
   - :error-message    custom error message
   - :operations       operation-specific rules
   - :inherit          inherit from parent (default false)
   - :cacheable        enable caching (default true)
   - :cache-duration   cache duration in seconds (default 300)"
  [& {:keys [roles strategy hierarchy description error-message operations
             inherit cacheable cache-duration]
      :or {roles [] strategy ROLE_MATCH_ANY hierarchy false
           description "" error-message "" operations ""
           inherit false cacheable true cache-duration 300}}]
  {:roles roles
   :strategy strategy
   :hierarchy hierarchy
   :description description
   :error-message error-message
   :operations operations
   :inherit inherit
   :cacheable cacheable
   :cache-duration-seconds cache-duration})

(defn authz-policy-config
  "Create an authorization policy configuration

   Arguments:
   - name             policy name (required)

   Options:
   - :type            policy type (rbac/abac/custom/hybrid, default :custom)
   - :description     policy description
   - :rule            authorization rule
   - :attributes      ABAC attributes (default [])
   - :cacheable       enable caching (default true)
   - :cache-duration  cache duration in seconds (default 300)
   - :recursive       apply recursively (default false)
   - :operations      operation-specific rules
   - :audit-logging   enable audit logging (default false)
   - :error-message   custom error message"
  [name & {:keys [type description rule attributes cacheable cache-duration
                  recursive operations audit-logging error-message]
           :or {type AUTHZ_POLICY_CUSTOM description "" rule ""
                attributes [] cacheable true cache-duration 300
                recursive false operations "" audit-logging false error-message ""}}]
  {:name name
   :type type
   :description description
   :rule rule
   :attributes attributes
   :cacheable cacheable
   :cache-duration-seconds cache-duration
   :recursive recursive
   :operations operations
   :audit-logging audit-logging
   :error-message error-message})

;; Builder functions
(defn authorize-builder
  "Create a custom authorization configuration using fluent builder pattern"
  []
  {:rule "" :policy "" :description "" :error-message "" :recursive false
   :operations "" :cacheable true :cache-duration-seconds 300})

(defn authorize
  "Fluent builder for custom authorization rules

   Example:
   (-> (authorize-builder)
       (assoc :rule \"isOwner(...)\")
       (assoc :description \"Ownership check\")
       authorize-config)"
  [builder]
  (authorize-config :rule (:rule builder)
                    :policy (:policy builder)
                    :description (:description builder)
                    :error-message (:error-message builder)
                    :recursive (:recursive builder)
                    :operations (:operations builder)
                    :cacheable (:cacheable builder)
                    :cache-duration (:cache-duration-seconds builder)))

(defn role-required-builder
  "Create a role-based access control configuration using fluent builder pattern"
  []
  {:roles [] :strategy ROLE_MATCH_ANY :hierarchy false
   :description "" :error-message "" :operations ""
   :inherit false :cacheable true :cache-duration-seconds 300})

(defn build-roles
  "Build role-based access control configuration"
  [builder]
  (role-required-config :roles (:roles builder)
                        :strategy (:strategy builder)
                        :hierarchy (:hierarchy builder)
                        :description (:description builder)
                        :error-message (:error-message builder)
                        :operations (:operations builder)
                        :inherit (:inherit builder)
                        :cacheable (:cacheable builder)
                        :cache-duration (:cache-duration-seconds builder)))

(defn authz-policy-builder
  "Create an authorization policy configuration using fluent builder pattern"
  [name]
  {:name name :type AUTHZ_POLICY_CUSTOM :description "" :rule ""
   :attributes [] :cacheable true :cache-duration-seconds 300
   :recursive false :operations "" :audit-logging false :error-message ""})

(defn build-policy
  "Build authorization policy configuration"
  [builder]
  (authz-policy-config (:name builder)
                       :type (:type builder)
                       :description (:description builder)
                       :rule (:rule builder)
                       :attributes (:attributes builder)
                       :cacheable (:cacheable builder)
                       :cache-duration (:cache-duration-seconds builder)
                       :recursive (:recursive builder)
                       :operations (:operations builder)
                       :audit-logging (:audit-logging builder)
                       :error-message (:error-message builder)))

;; Helper functions for strategy handling
(defn strategy-value [strategy]
  (case strategy
    :any "any"
    :all "all"
    :exactly "exactly"
    (str strategy)))

(defn policy-type-value [policy-type]
  (case policy-type
    :rbac "rbac"
    :abac "abac"
    :custom "custom"
    :hybrid "hybrid"
    (str policy-type)))

;; Serialization helpers
(defn authorize-config->map [config]
  {:rule (:rule config)
   :policy (:policy config)
   :description (:description config)
   :errorMessage (:error-message config)
   :recursive (:recursive config)
   :operations (:operations config)
   :cacheable (:cacheable config)
   :cacheDurationSeconds (:cache-duration-seconds config)})

(defn role-required-config->map [config]
  {:roles (:roles config)
   :strategy (strategy-value (:strategy config))
   :hierarchy (:hierarchy config)
   :description (:description config)
   :errorMessage (:error-message config)
   :operations (:operations config)
   :inherit (:inherit config)
   :cacheable (:cacheable config)
   :cacheDurationSeconds (:cache-duration-seconds config)})

(defn authz-policy-config->map [config]
  {:name (:name config)
   :type (policy-type-value (:type config))
   :description (:description config)
   :rule (:rule config)
   :attributes (:attributes config)
   :cacheable (:cacheable config)
   :cacheDurationSeconds (:cache-duration-seconds config)
   :recursive (:recursive config)
   :operations (:operations config)
   :auditLogging (:audit-logging config)
   :errorMessage (:error-message config)})
