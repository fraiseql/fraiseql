(ns fraiseql.schema
  (:require [cheshire.core :as json]
            [clojure.java.io :as io])
  (:import [java.nio.file Files Paths]))

;; Central registry for GraphQL type definitions
(def ^:private registry (atom {}))

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
                                                  {:name field-name
                                                   :type (get field-def :type "String")
                                                   :nullable (get field-def :nullable false)})
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

(defn get-type-names
  "Get all registered type names"
  []
  (vec (keys @registry)))
