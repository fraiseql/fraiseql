(ns fraiseql.phase18-cycle22-scope-extraction-test
  (:require [clojure.test :refer [deftest is testing]]
            [fraiseql.schema :as schema]
            [cheshire.core :as json]))

(defn setup [] (schema/reset))
(defn teardown [] (schema/reset))

;; MARK: - Field Creation Tests (3 tests)

(deftest field-creation-all-properties
  (testing "field should create with all properties"
    (setup)
    (schema/register-type "User"
      {:email {:type "String"
               :nullable false
               :description "User email address"
               :requires_scope "read:user.email"}})

    (let [type-info (schema/get-type "User")]
      (is (not (nil? type-info)))
      (let [field-config (get-in type-info [:fields :email])]
        (is (= "String" (:type field-config)))
        (is (= false (:nullable field-config)))
        (is (= "User email address" (:description field-config)))
        (is (= "read:user.email" (:requires_scope field-config)))))
    (teardown)))

(deftest field-creation-minimal-properties
  (testing "field should create with minimal properties"
    (setup)
    (schema/register-type "User"
      {:id {:type "Int"}})

    (let [type-info (schema/get-type "User")]
      (is (not (nil? type-info)))
      (let [field-config (get-in type-info [:fields :id])]
        (is (= "Int" (:type field-config)))
        (is (nil? (:requires_scope field-config)))
        (is (nil? (:requires_scopes field-config)))))
    (teardown)))

(deftest field-metadata-preservation
  (testing "field should preserve metadata alongside scopes"
    (setup)
    (schema/register-type "User"
      {:password {:type "String"
                  :nullable false
                  :description "Hashed password"
                  :requires_scope "admin:user.*"}})

    (let [type-info (schema/get-type "User")]
      (let [field-config (get-in type-info [:fields :password])]
        (is (= "String" (:type field-config)))
        (is (= "admin:user.*" (:requires_scope field-config)))
        (is (= "Hashed password" (:description field-config)))))
    (teardown)))

;; MARK: - Single Scope Requirement Tests (3 tests)

(deftest single-scope-format
  (testing "field should support single scope format"
    (setup)
    (schema/register-type "User"
      {:email {:type "String"
               :requires_scope "read:user.email"}})

    (let [type-info (schema/get-type "User")]
      (let [field-config (get-in type-info [:fields :email])]
        (is (= "read:user.email" (:requires_scope field-config)))
        (is (nil? (:requires_scopes field-config)))))
    (teardown)))

(deftest wildcard-resource-scope
  (testing "field should support wildcard resource scope"
    (setup)
    (schema/register-type "User"
      {:profile {:type "Object"
                 :requires_scope "read:User.*"}})

    (let [type-info (schema/get-type "User")]
      (let [field-config (get-in type-info [:fields :profile])]
        (is (= "read:User.*" (:requires_scope field-config)))))
    (teardown)))

(deftest global-wildcard-scope
  (testing "field should support global wildcard scope"
    (setup)
    (schema/register-type "User"
      {:secret {:type "String"
                :requires_scope "admin:*"}})

    (let [type-info (schema/get-type "User")]
      (let [field-config (get-in type-info [:fields :secret])]
        (is (= "admin:*" (:requires_scope field-config)))))
    (teardown)))

;; MARK: - Multiple Scopes Array Tests (3 tests)

(deftest multiple-scopes-array
  (testing "field should support multiple scopes array"
    (setup)
    (schema/register-type "User"
      {:email {:type "String"
               :requires_scopes ["read:user.email" "write:user.email"]}})

    (let [type-info (schema/get-type "User")]
      (let [scopes (get-in type-info [:fields :email :requires_scopes])]
        (is (not (nil? scopes)))
        (is (= 2 (count scopes)))
        (is (some #(= "read:user.email" %) scopes))
        (is (some #(= "write:user.email" %) scopes))))
    (teardown)))

(deftest single-element-scopes-array
  (testing "field should support single element scopes array"
    (setup)
    (schema/register-type "User"
      {:profile {:type "Object"
                 :requires_scopes ["read:user.profile"]}})

    (let [type-info (schema/get-type "User")]
      (let [scopes (get-in type-info [:fields :profile :requires_scopes])]
        (is (not (nil? scopes)))
        (is (= 1 (count scopes)))
        (is (= "read:user.profile" (first scopes)))))
    (teardown)))

(deftest complex-scopes-array
  (testing "field should support complex scopes array"
    (setup)
    (schema/register-type "User"
      {:data {:type "String"
              :requires_scopes ["read:user.email" "write:user.*" "admin:*"]}})

    (let [type-info (schema/get-type "User")]
      (let [scopes (get-in type-info [:fields :data :requires_scopes])]
        (is (not (nil? scopes)))
        (is (= 3 (count scopes)))))
    (teardown)))

;; MARK: - Scope Pattern Validation Tests (6 tests)

(deftest validate-specific-field-scope
  (testing "scope validator should validate specific field scope"
    (setup)
    (is (not (nil?
      (schema/register-type "User"
        {:email {:type "String"
                 :requires_scope "read:user.email"}}))))
    (teardown)))

(deftest validate-resource-wildcard-scope
  (testing "scope validator should validate resource wildcard scope"
    (setup)
    (is (not (nil?
      (schema/register-type "User"
        {:profile {:type "Object"
                   :requires_scope "read:User.*"}}))))
    (teardown)))

(deftest validate-global-admin-wildcard
  (testing "scope validator should validate global admin wildcard"
    (setup)
    (is (not (nil?
      (schema/register-type "User"
        {:secret {:type "String"
                  :requires_scope "admin:*"}}))))
    (teardown)))

(deftest reject-scope-missing-colon
  (testing "scope validator should reject scope missing colon"
    (setup)
    (is (thrown? Exception
      (schema/register-type "User"
        {:data {:type "String"
                :requires_scope "readuser"}})))
    (teardown)))

(deftest reject-action-with-hyphen
  (testing "scope validator should reject action with hyphen"
    (setup)
    (is (thrown? Exception
      (schema/register-type "User"
        {:data {:type "String"
                :requires_scope "read-all:user"}})))
    (teardown)))

(deftest reject-resource-with-hyphen
  (testing "scope validator should reject resource with hyphen"
    (setup)
    (is (thrown? Exception
      (schema/register-type "User"
        {:data {:type "String"
                :requires_scope "read:user-data"}})))
    (teardown)))

;; MARK: - Schema Registry Tests (3 tests)

(deftest register-type-with-scopes
  (testing "schema should register type with fields and scopes"
    (setup)
    (schema/register-type "User"
      {:id {:type "Int" :nullable false}
       :email {:type "String"
               :nullable false
               :requires_scope "read:user.email"}})

    (let [type-names (schema/get-type-names)]
      (is (some #(= "User" %) type-names)))
    (teardown)))

(deftest extract-scoped-fields
  (testing "schema should extract scoped fields from registry"
    (setup)
    (schema/register-type "User"
      {:id {:type "Int" :nullable false}
       :email {:type "String"
               :nullable false
               :requires_scope "read:user.email"}
       :password {:type "String"
                  :nullable false
                  :requires_scope "admin:user.password"}})

    (let [type-info (schema/get-type "User")]
      (is (not (nil? type-info)))
      (is (= "read:user.email" (get-in type-info [:fields :email :requires_scope])))
      (is (= "admin:user.password" (get-in type-info [:fields :password :requires_scope]))))
    (teardown)))

(deftest multiple-types-different-scopes
  (testing "schema should handle multiple types with different scopes"
    (setup)
    (schema/register-type "User"
      {:id {:type "Int"}
       :email {:type "String"
               :requires_scope "read:user.email"}})

    (schema/register-type "Post"
      {:id {:type "Int"}
       :content {:type "String"
                 :requires_scope "read:post.content"}})

    (let [type-names (schema/get-type-names)]
      (is (= 2 (count type-names)))
      (is (some #(= "User" %) type-names))
      (is (some #(= "Post" %) type-names)))
    (teardown)))

;; MARK: - JSON Export Tests (2 tests)

(deftest export-scope-in-json
  (testing "schema export should include scope in field JSON"
    (setup)
    (schema/register-type "User"
      {:email {:type "String"
               :nullable false
               :requires_scope "read:user.email"}})

    (let [json-str (schema/export-types false)]
      (is (clojure.string/includes? json-str "User"))
      (is (clojure.string/includes? json-str "email"))
      (is (clojure.string/includes? json-str "read:user.email"))
      (is (clojure.string/includes? json-str "requires_scope")))
    (teardown)))

(deftest export-multiple-types-with-scopes
  (testing "schema export should export multiple types with scopes"
    (setup)
    (schema/register-type "User"
      {:id {:type "Int"}
       :email {:type "String"
               :requires_scope "read:user.email"}})

    (schema/register-type "Post"
      {:id {:type "Int"}
       :content {:type "String"
                 :requires_scope "read:post.content"}})

    (let [json-str (schema/export-types false)]
      (is (clojure.string/includes? json-str "User"))
      (is (clojure.string/includes? json-str "Post"))
      (is (clojure.string/includes? json-str "read:user.email"))
      (is (clojure.string/includes? json-str "read:post.content")))
    (teardown)))

;; MARK: - Conflicting Scope/Scopes Tests (2 tests)

(deftest reject-both-scope-and-scopes
  (testing "field with both scope and scopes should be rejected"
    (setup)
    (is (thrown? Exception
      (schema/register-type "User"
        {:email {:type "String"
                 :requires_scope "read:user.email"
                 :requires_scopes ["write:user.email"]}})))
    (teardown)))

(deftest reject-empty-scope-string
  (testing "scope validator should reject empty scope string"
    (setup)
    (is (thrown? Exception
      (schema/register-type "User"
        {:data {:type "String"
                :requires_scope ""}})))
    (teardown)))
