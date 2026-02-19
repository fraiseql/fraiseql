(ns fraiseql.schema
  (:require [cheshire.core :as json]
            [clojure.java.io :as io])
  (:import [java.nio.file Files Paths]))

;; Central registry for GraphQL type definitions
(def ^:private registry (atom {}))

;; MARK: - Scope Validation

(defn validate-scope
  "Validates scope format: action:resource
   Examples: read:user.email, admin:*, write:Post.*
   Rules:
   - Action: [a-zA-Z_][a-zA-Z0-9_]*
   - Resource: [a-zA-Z_][a-zA-Z0-9_.]*|*
   - Global wildcard: *"
  [scope]
  (cond
    (nil? scope) false
    (= "" scope) false
    (= "*" scope) true
    :else
    (let [parts (clojure.string/split scope #":" 2)]
      (if (= 2 (count parts))
        (let [[action resource] parts]
          (and (not (empty? action))
               (not (empty? resource))
               (re-matches #"[a-zA-Z_][a-zA-Z0-9_]*" action)
               (or (= "*" resource)
                   (re-matches #"[a-zA-Z_][a-zA-Z0-9_.]*" resource))))
        false))))

(defn validate-field-scopes
  "Validates scope fields in all fields of a type"
  [fields type-name]
  (doseq [[field-name field-config] fields]
    (let [has-scope (contains? field-config :requires_scope)
          has-scopes (contains? field-config :requires_scopes)]

      ;; Check for conflicting scope and scopes
      (when (and has-scope has-scopes)
        (throw (Exception. (str "Field \"" field-name "\" cannot have both requires_scope and requires_scopes"))))

      ;; Validate requires_scope if present
      (when has-scope
        (let [scope (:requires_scope field-config)]
          (if (not (string? scope))
            (throw (Exception. (str "Field \"" field-name "\" requires_scope must be a string"))))
          (if (not (validate-scope scope))
            (throw (Exception. (str "Field \"" field-name "\" has invalid scope format: \"" scope "\""))))))

      ;; Validate requires_scopes if present
      (when has-scopes
        (let [scopes (:requires_scopes field-config)]
          (if (not (vector? scopes))
            (throw (Exception. (str "Field \"" field-name "\" requires_scopes must be a vector"))))
          (if (empty? scopes)
            (throw (Exception. (str "Field \"" field-name "\" requires_scopes cannot be empty"))))
          (doseq [scope scopes]
            (if (not (string? scope))
              (throw (Exception. (str "Field \"" field-name "\" requires_scopes contains non-string value"))))
            (if (not (validate-scope scope))
              (throw (Exception. (str "Field \"" field-name "\" has invalid scope in requires_scopes: \"" scope "\"")))))))))))

(defn register
  "Register a type definition in the schema registry"
  [name type-info]
  (swap! registry assoc name type-info))

(defn get-type-names
  "Get all registered type names"
  []
  (keys @registry))

(defn get-type
  "Get a registered type by name"
  [name]
  (get @registry name))

(defn clear
  "Clear all registered types (useful for testing)"
  []
  (reset! registry {}))

(defn register-type
  "Register a type definition

   name - The type name
   fields - Map of field name to field definition
   description - Optional type description"
  ([name fields]
   (register-type name fields nil))
  ([name fields description]
   ;; Validate scope fields in all fields
   (validate-field-scopes fields name)

   (register name {:name name
                   :fields fields
                   :description description})))

(defn export-types
  "Export minimal schema with only types (TOML workflow)

   Returns JSON containing only the \"types\" section.
   All operational configuration (queries, mutations, federation, security, observers)
   comes from fraiseql.toml and is merged during compilation.

   pretty - Pretty-print JSON (true = formatted, false = compact)"
  ([]
   (export-types true))
  ([pretty]
   (let [type-names (get-type-names)
         types (map (fn [type-name]
                      (when-let [type-info (get-type type-name)]
                        (let [fields (:fields type-info)
                              fields-array (map (fn [[field-name field-def]]
                                                  (let [field {:name field-name
                                                               :type (get field-def :type "String")
                                                               :nullable (get field-def :nullable false)}]
                                                    (cond-> field
                                                      (contains? field-def :requires_scope)
                                                      (assoc :requires_scope (:requires_scope field-def))
                                                      (contains? field-def :requires_scopes)
                                                      (assoc :requires_scopes (:requires_scopes field-def)))))
                                                fields)]
                          (cond-> {:name type-name
                                   :fields (vec fields-array)}
                            (:description type-info)
                            (assoc :description (:description type-info))))))
                    type-names)
         types (vec (filter identity types))
         minimal-schema {:types types}]
     (if pretty
       (json/generate-string minimal-schema {:pretty true})
       (json/generate-string minimal-schema)))))

(defn export-types-file
  "Export minimal types to a file

   output-path - File path for types.json"
  [output-path]
  (try
    (let [types-json (export-types true)
          path (Paths/get output-path (into-array String []))
          parent (.getParent path)]

      ;; Ensure directory exists
      (when parent
        (Files/createDirectories parent (into-array java.nio.file.attribute.FileAttribute [])))

      ;; Write file
      (spit output-path types-json)

      ;; Print summary
      (let [types-count (count (get-type-names))]
        (println (str "✅ Types exported to " output-path))
        (println (str "   Types: " types-count))
        (println)
        (println "🎯 Next steps:")
        (println (str "   1. fraiseql compile fraiseql.toml --types " output-path))
        (println "   2. This merges types with TOML configuration")
        (println "   3. Result: schema.compiled.json with types + all config")))
    (catch Exception e
      (throw (RuntimeException. (str "Failed to write types file: " output-path))))))

(defn reset
  "Reset schema registry (useful for testing)"
  []
  (clear))
