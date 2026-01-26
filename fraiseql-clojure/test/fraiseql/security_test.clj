(ns fraiseql.security-test
  (:require [clojure.test :refer :all]
            [fraiseql.security :as security]))

;; ============================================================================
;; AUTHORIZATION TESTS (11 tests)
;; ============================================================================

(deftest test-simple-authorization-rule
  (let [config (-> (security/authorize-builder)
                   (assoc :rule "isOwner($context.userId, $field.ownerId)")
                   (assoc :description "Ownership check")
                   security/authorize)]
    (is (= "isOwner($context.userId, $field.ownerId)" (:rule config)))
    (is (= "Ownership check" (:description config)))))

(deftest test-authorization-with-policy
  (let [config (-> (security/authorize-builder)
                   (assoc :policy "ownerOnly")
                   (assoc :description "References named policy")
                   security/authorize)]
    (is (= "ownerOnly" (:policy config)))))

(deftest test-fluent-chaining
  (let [config (-> (security/authorize-builder)
                   (assoc :rule "hasPermission($context)")
                   (assoc :description "Complex rule")
                   (assoc :error-message "Access denied")
                   (assoc :recursive true)
                   (assoc :operations "read")
                   security/authorize)]
    (is (= "hasPermission($context)" (:rule config)))
    (is (= true (:recursive config)))
    (is (= "read" (:operations config)))))

(deftest test-caching-configuration
  (let [config (-> (security/authorize-builder)
                   (assoc :rule "checkAccess($context)")
                   (assoc :cacheable true)
                   (assoc :cache-duration-seconds 600)
                   security/authorize)]
    (is (= true (:cacheable config)))
    (is (= 600 (:cache-duration-seconds config)))))

(deftest test-error-message
  (let [config (-> (security/authorize-builder)
                   (assoc :rule "adminOnly($context)")
                   (assoc :error-message "Only administrators can access this")
                   security/authorize)]
    (is (= "Only administrators can access this" (:error-message config)))))

(deftest test-recursive
  (let [config (-> (security/authorize-builder)
                   (assoc :rule "checkNested($context)")
                   (assoc :recursive true)
                   (assoc :description "Applied to nested types")
                   security/authorize)]
    (is (= true (:recursive config)))))

(deftest test-operation-specific
  (let [config (-> (security/authorize-builder)
                   (assoc :rule "canDelete($context)")
                   (assoc :operations "delete")
                   (assoc :description "Only applies to delete operations")
                   security/authorize)]
    (is (= "delete" (:operations config)))))

(deftest test-to-map
  (let [config (-> (security/authorize-builder)
                   (assoc :rule "testRule")
                   (assoc :description "Test")
                   security/authorize)
        map-result (security/authorize-config->map config)]
    (is (= "testRule" (:rule map-result)))
    (is (= "Test" (:description map-result)))))

(deftest test-multiple-configurations
  (let [config1 (security/authorize-config :rule "rule1")
        config2 (security/authorize-config :rule "rule2")]
    (is (not= (:rule config1) (:rule config2)))))

(deftest test-default-cache-settings
  (let [config (security/authorize-config :rule "test")]
    (is (= true (:cacheable config)))
    (is (= 300 (:cache-duration-seconds config)))))

(deftest test-all-options
  (let [config (-> (security/authorize-builder)
                   (assoc :rule "complex")
                   (assoc :policy "policy")
                   (assoc :description "Complex authorization")
                   (assoc :error-message "Error")
                   (assoc :recursive true)
                   (assoc :operations "create,read,update,delete")
                   (assoc :cacheable false)
                   (assoc :cache-duration-seconds 1000)
                   security/authorize)]
    (is (= "complex" (:rule config)))
    (is (= false (:cacheable config)))
    (is (= 1000 (:cache-duration-seconds config)))))

;; ============================================================================
;; ROLE BASED ACCESS CONTROL TESTS (18 tests)
;; ============================================================================

(deftest test-single-role-requirement
  (let [config (security/role-required-config :roles ["admin"])]
    (is (= 1 (count (:roles config))))
    (is (= "admin" (first (:roles config))))))

(deftest test-multiple-role-requirements
  (let [config (security/role-required-config :roles ["manager" "director"])]
    (is (= 2 (count (:roles config))))
    (is (some #{"manager"} (:roles config)))
    (is (some #{"director"} (:roles config)))))

(deftest test-any-role-strategy
  (let [config (security/role-required-config
                 :roles ["viewer" "editor"]
                 :strategy security/ROLE_MATCH_ANY)]
    (is (= security/ROLE_MATCH_ANY (:strategy config)))))

(deftest test-all-role-strategy
  (let [config (security/role-required-config
                 :roles ["admin" "auditor"]
                 :strategy security/ROLE_MATCH_ALL)]
    (is (= security/ROLE_MATCH_ALL (:strategy config)))))

(deftest test-exactly-role-strategy
  (let [config (security/role-required-config
                 :roles ["exact_role"]
                 :strategy security/ROLE_MATCH_EXACTLY)]
    (is (= security/ROLE_MATCH_EXACTLY (:strategy config)))))

(deftest test-role-hierarchy
  (let [config (security/role-required-config :roles ["admin"] :hierarchy true)]
    (is (= true (:hierarchy config)))))

(deftest test-role-inheritance
  (let [config (security/role-required-config :roles ["editor"] :inherit true)]
    (is (= true (:inherit config)))))

(deftest test-operation-specific-roles
  (let [config (security/role-required-config :roles ["editor"] :operations "create,update")]
    (is (= "create,update" (:operations config)))))

(deftest test-role-error-message
  (let [config (security/role-required-config
                 :roles ["admin"]
                 :error-message "Administrator access required")]
    (is (= "Administrator access required" (:error-message config)))))

(deftest test-role-caching
  (let [config (security/role-required-config
                 :roles ["viewer"]
                 :cacheable true
                 :cache-duration 1800)]
    (is (= true (:cacheable config)))
    (is (= 1800 (:cache-duration-seconds config)))))

(deftest test-admin-pattern
  (let [config (security/role-required-config
                 :roles ["admin"]
                 :strategy security/ROLE_MATCH_ANY
                 :description "Admin access")]
    (is (= 1 (count (:roles config))))
    (is (= "admin" (first (:roles config))))))

(deftest test-manager-director-pattern
  (let [config (security/role-required-config
                 :roles ["manager" "director"]
                 :strategy security/ROLE_MATCH_ANY)]
    (is (= 2 (count (:roles config))))
    (is (= security/ROLE_MATCH_ANY (:strategy config)))))

(deftest test-data-scientist-pattern
  (let [config (security/role-required-config
                 :roles ["data_scientist" "analyst"]
                 :strategy security/ROLE_MATCH_ANY)]
    (is (= 2 (count (:roles config))))))

(deftest test-role-to-map
  (let [config (security/role-required-config
                 :roles ["admin" "editor"]
                 :strategy security/ROLE_MATCH_ANY)
        map-result (security/role-required-config->map config)]
    (is (= "any" (:strategy map-result)))))

(deftest test-role-description
  (let [config (security/role-required-config
                 :roles ["viewer"]
                 :description "Read-only access")]
    (is (= "Read-only access" (:description config)))))

(deftest test-role-default-values
  (let [config (security/role-required-config :roles ["user"])]
    (is (= false (:hierarchy config)))
    (is (= false (:inherit config)))
    (is (= true (:cacheable config)))
    (is (= 300 (:cache-duration-seconds config)))))

;; ============================================================================
;; ATTRIBUTE BASED ACCESS CONTROL TESTS (16 tests)
;; ============================================================================

(deftest test-abac-policy-creation
  (let [config (security/authz-policy-config "accessControl"
                 :type security/AUTHZ_POLICY_ABAC
                 :attributes ["clearance_level >= 2"]
                 :description "Basic clearance")]
    (is (= "accessControl" (:name config)))
    (is (= security/AUTHZ_POLICY_ABAC (:type config)))))

(deftest test-multiple-attributes
  (let [config (security/authz-policy-config "secretAccess"
                 :type security/AUTHZ_POLICY_ABAC
                 :attributes ["clearance_level >= 3" "background_check == true"])]
    (is (= 2 (count (:attributes config))))))

(deftest test-clearance-level-policy
  (let [config (security/authz-policy-config "topSecret"
                 :type security/AUTHZ_POLICY_ABAC
                 :attributes ["clearance_level >= 3"])]
    (is (= 1 (count (:attributes config))))))

(deftest test-department-policy
  (let [config (security/authz-policy-config "financeDept"
                 :type security/AUTHZ_POLICY_ABAC
                 :attributes ["department == \"finance\""])]
    (is (= "financeDept" (:name config)))))

(deftest test-time-based-policy
  (let [config (security/authz-policy-config "businessHours"
                 :type security/AUTHZ_POLICY_ABAC
                 :attributes ["now >= 9:00 AM" "now <= 5:00 PM"])]
    (is (= 2 (count (:attributes config))))))

(deftest test-geographic-policy
  (let [config (security/authz-policy-config "usOnly"
                 :type security/AUTHZ_POLICY_ABAC
                 :attributes ["country == \"US\""])]
    (is (= 1 (count (:attributes config))))))

(deftest test-gdpr-policy
  (let [config (security/authz-policy-config "gdprCompliance"
                 :type security/AUTHZ_POLICY_ABAC
                 :attributes ["gdpr_compliant == true" "data_residency == \"EU\""])]
    (is (= 2 (count (:attributes config))))))

(deftest test-data-classification-policy
  (let [config (security/authz-policy-config "classifiedData"
                 :type security/AUTHZ_POLICY_ABAC
                 :attributes ["classification >= 2"])]
    (is (= 1 (count (:attributes config))))))

(deftest test-abac-caching
  (let [config (security/authz-policy-config "cachedAccess"
                 :type security/AUTHZ_POLICY_ABAC
                 :attributes ["role == \"viewer\""]
                 :cacheable true
                 :cache-duration 600)]
    (is (= true (:cacheable config)))
    (is (= 600 (:cache-duration-seconds config)))))

(deftest test-abac-audit-logging
  (let [config (security/authz-policy-config "auditedAccess"
                 :type security/AUTHZ_POLICY_ABAC
                 :attributes ["audit_enabled == true"]
                 :audit-logging true)]
    (is (= true (:audit-logging config)))))

(deftest test-abac-recursive
  (let [config (security/authz-policy-config "recursiveAccess"
                 :type security/AUTHZ_POLICY_ABAC
                 :attributes ["permission >= 1"]
                 :recursive true)]
    (is (= true (:recursive config)))))

(deftest test-abac-operation-specific
  (let [config (security/authz-policy-config "readOnly"
                 :type security/AUTHZ_POLICY_ABAC
                 :attributes ["can_read == true"]
                 :operations "read")]
    (is (= "read" (:operations config)))))

(deftest test-complex-abac-policy
  (let [config (security/authz-policy-config "complex"
                 :type security/AUTHZ_POLICY_ABAC
                 :attributes ["level >= 2" "verified == true" "active == true"]
                 :audit-logging true
                 :cacheable true)]
    (is (= 3 (count (:attributes config))))
    (is (= true (:audit-logging config)))))

(deftest test-abac-error-message
  (let [config (security/authz-policy-config "restricted"
                 :type security/AUTHZ_POLICY_ABAC
                 :attributes ["clearance >= 2"]
                 :error-message "Insufficient clearance level")]
    (is (= "Insufficient clearance level" (:error-message config)))))

(deftest test-abac-to-map
  (let [config (security/authz-policy-config "test"
                 :type security/AUTHZ_POLICY_ABAC
                 :attributes ["test >= 1"])
        map-result (security/authz-policy-config->map config)]
    (is (= "abac" (:type map-result)))))

(deftest test-abac-default-values
  (let [config (security/authz-policy-config "default"
                 :type security/AUTHZ_POLICY_ABAC)]
    (is (= true (:cacheable config)))
    (is (= 300 (:cache-duration-seconds config)))
    (is (= false (:recursive config)))))

;; ============================================================================
;; AUTHORIZATION POLICY TESTS (19 tests)
;; ============================================================================

(deftest test-rbac-policy
  (let [config (security/authz-policy-config "adminOnly"
                 :type security/AUTHZ_POLICY_RBAC
                 :rule "hasRole($context, 'admin')"
                 :description "Access restricted to administrators"
                 :audit-logging true)]
    (is (= "adminOnly" (:name config)))
    (is (= security/AUTHZ_POLICY_RBAC (:type config)))
    (is (= "hasRole($context, 'admin')" (:rule config)))
    (is (= true (:audit-logging config)))))

(deftest test-abac-policy-full
  (let [config (security/authz-policy-config "secretClearance"
                 :type security/AUTHZ_POLICY_ABAC
                 :description "Requires top secret clearance"
                 :attributes ["clearance_level >= 3" "background_check == true"])]
    (is (= "secretClearance" (:name config)))
    (is (= security/AUTHZ_POLICY_ABAC (:type config)))
    (is (= 2 (count (:attributes config))))))

(deftest test-custom-policy
  (let [config (security/authz-policy-config "customRule"
                 :type security/AUTHZ_POLICY_CUSTOM
                 :rule "isOwner($context.userId, $resource.ownerId)")]
    (is (= security/AUTHZ_POLICY_CUSTOM (:type config)))))

(deftest test-hybrid-policy
  (let [config (security/authz-policy-config "auditAccess"
                 :type security/AUTHZ_POLICY_HYBRID
                 :rule "hasRole($context, 'auditor')"
                 :attributes ["audit_enabled == true"])]
    (is (= security/AUTHZ_POLICY_HYBRID (:type config)))
    (is (= "hasRole($context, 'auditor')" (:rule config)))))

(deftest test-multiple-policies
  (let [p1 (security/authz-policy-config "policy1" :type security/AUTHZ_POLICY_RBAC)
        p2 (security/authz-policy-config "policy2" :type security/AUTHZ_POLICY_ABAC)
        p3 (security/authz-policy-config "policy3" :type security/AUTHZ_POLICY_CUSTOM)]
    (is (= "policy1" (:name p1)))
    (is (= "policy2" (:name p2)))
    (is (= "policy3" (:name p3)))))

(deftest test-pii-access-policy
  (let [config (security/authz-policy-config "piiAccess"
                 :type security/AUTHZ_POLICY_RBAC
                 :rule "hasRole($context, 'data_manager')")]
    (is (= "piiAccess" (:name config)))))

(deftest test-admin-only-policy
  (let [config (security/authz-policy-config "adminOnly"
                 :type security/AUTHZ_POLICY_RBAC
                 :audit-logging true)]
    (is (= true (:audit-logging config)))))

(deftest test-recursive-policy
  (let [config (security/authz-policy-config "recursiveProtection"
                 :type security/AUTHZ_POLICY_CUSTOM
                 :recursive true)]
    (is (= true (:recursive config)))))

(deftest test-operation-specific-policy
  (let [config (security/authz-policy-config "readOnly"
                 :type security/AUTHZ_POLICY_CUSTOM
                 :operations "read")]
    (is (= "read" (:operations config)))))

(deftest test-cached-policy
  (let [config (security/authz-policy-config "cachedAccess"
                 :type security/AUTHZ_POLICY_CUSTOM
                 :cacheable true
                 :cache-duration 3600)]
    (is (= true (:cacheable config)))
    (is (= 3600 (:cache-duration-seconds config)))))

(deftest test-audited-policy
  (let [config (security/authz-policy-config "auditedAccess"
                 :type security/AUTHZ_POLICY_RBAC
                 :audit-logging true)]
    (is (= true (:audit-logging config)))))

(deftest test-policy-with-error-message
  (let [config (security/authz-policy-config "restrictedAccess"
                 :type security/AUTHZ_POLICY_RBAC
                 :error-message "Only executive level users can access this resource")]
    (is (= "Only executive level users can access this resource" (:error-message config)))))

(deftest test-policy-fluent-chaining
  (let [builder (-> (security/authz-policy-builder "complexPolicy")
                    (assoc :type security/AUTHZ_POLICY_HYBRID)
                    (assoc :rule "hasRole($context, 'admin')")
                    (assoc :attributes ["security_clearance >= 3"])
                    (assoc :cacheable true)
                    (assoc :cache-duration-seconds 1800)
                    (assoc :recursive false)
                    (assoc :operations "create,update,delete")
                    (assoc :audit-logging true)
                    (assoc :error-message "Insufficient privileges"))
        config (security/build-policy builder)]
    (is (= "complexPolicy" (:name config)))
    (is (= security/AUTHZ_POLICY_HYBRID (:type config)))
    (is (= true (:cacheable config)))
    (is (= true (:audit-logging config)))))

(deftest test-policy-composition
  (let [p1 (security/authz-policy-config "publicAccess" :type security/AUTHZ_POLICY_RBAC :rule "true")
        p2 (security/authz-policy-config "piiAccess" :type security/AUTHZ_POLICY_RBAC)
        p3 (security/authz-policy-config "adminAccess" :type security/AUTHZ_POLICY_RBAC)]
    (is (= "publicAccess" (:name p1)))
    (is (= "piiAccess" (:name p2)))
    (is (= "adminAccess" (:name p3)))))

(deftest test-financial-data-policy
  (let [config (security/authz-policy-config "financialData"
                 :type security/AUTHZ_POLICY_ABAC
                 :attributes ["clearance_level >= 2" "department == \"finance\""])]
    (is (= "financialData" (:name config)))
    (is (= 2 (count (:attributes config))))))

(deftest test-security-clearance-policy
  (let [config (security/authz-policy-config "secretClearance"
                 :type security/AUTHZ_POLICY_ABAC
                 :attributes ["clearance_level >= 3" "background_check == true"])]
    (is (= 2 (count (:attributes config))))))

(deftest test-default-policy
  (let [config (security/authz-policy-config "default")]
    (is (= "default" (:name config)))
    (is (= security/AUTHZ_POLICY_CUSTOM (:type config)))
    (is (= true (:cacheable config)))
    (is (= 300 (:cache-duration-seconds config)))))

(deftest test-policy-to-map
  (let [config (security/authz-policy-config "test"
                 :type security/AUTHZ_POLICY_RBAC
                 :rule "test_rule")
        map-result (security/authz-policy-config->map config)]
    (is (= "test" (:name map-result)))
    (is (= "rbac" (:type map-result)))))
