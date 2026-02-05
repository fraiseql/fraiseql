(ns fraiseql.export-types-test
  (:require [clojure.test :refer [deftest is testing before after]]
            [fraiseql.schema :as schema]
            [cheshire.core :as json]
            [clojure.java.io :as io]))

(defn setup [] (schema/reset))
(defn teardown [] (schema/reset))

(deftest export-types-minimal-single-type
  (testing "should export minimal schema with single type"
    (setup)
    (schema/register-type "User"
      {:id {:type "ID" :nullable false}
       :name {:type "String" :nullable false}
       :email {:type "String" :nullable false}}
      "User in the system")

    (let [json (schema/export-types true)
          parsed (json/parse-string json)]

      (is (contains? parsed "types"))
      (is (vector? (get parsed "types")))
      (is (= 1 (count (get parsed "types"))))
      (is (not (contains? parsed "queries")))
      (is (not (contains? parsed "mutations")))
      (is (not (contains? parsed "observers")))
      (is (not (contains? parsed "authz_policies")))

      (let [user-def (first (get parsed "types"))]
        (is (= "User" (get user-def "name")))
        (is (= "User in the system" (get user-def "description")))))
    (teardown)))

(deftest export-types-multiple-types
  (testing "should export minimal schema with multiple types"
    (setup)
    (schema/register-type "User"
      {:id {:type "ID" :nullable false}
       :name {:type "String" :nullable false}})

    (schema/register-type "Post"
      {:id {:type "ID" :nullable false}
       :title {:type "String" :nullable false}
       :authorId {:type "ID" :nullable false}})

    (let [json (schema/export-types true)
          parsed (json/parse-string json)
          types (get parsed "types")]

      (is (= 2 (count types)))

      (let [type-names (map #(get % "name") types)]
        (is (some #(= "User" %) type-names))
        (is (some #(= "Post" %) type-names))))
    (teardown)))

(deftest export-types-no-queries
  (testing "should not include queries in minimal export"
    (setup)
    (schema/register-type "User"
      {:id {:type "ID" :nullable false}})

    (let [json (schema/export-types true)
          parsed (json/parse-string json)]

      (is (contains? parsed "types"))
      (is (not (contains? parsed "queries")))
      (is (not (contains? parsed "mutations"))))
    (teardown)))

(deftest export-types-compact-format
  (testing "should export compact format when pretty is false"
    (setup)
    (schema/register-type "User"
      {:id {:type "ID" :nullable false}})

    (let [compact (schema/export-types false)
          pretty (schema/export-types true)
          parsed (json/parse-string compact)]

      (is (contains? parsed "types"))
      (is (<= (count compact) (count pretty))))
    (teardown)))

(deftest export-types-pretty-format
  (testing "should export pretty format when pretty is true"
    (setup)
    (schema/register-type "User"
      {:id {:type "ID" :nullable false}})

    (let [json (schema/export-types true)]
      (is (clojure.string/includes? json "\n")))
    (teardown)))

(deftest export-types-to-file
  (testing "should export types to file"
    (setup)
    (schema/register-type "User"
      {:id {:type "ID" :nullable false}
       :name {:type "String" :nullable false}})

    (let [tmp-file "/tmp/fraiseql_types_test_clojure.json"]
      (when (.exists (io/file tmp-file))
        (io/delete-file tmp-file))

      (schema/export-types-file tmp-file)

      (is (.exists (io/file tmp-file)))

      (let [content (slurp tmp-file)
            parsed (json/parse-string content)]
        (is (contains? parsed "types"))
        (is (= 1 (count (get parsed "types")))))

      (io/delete-file tmp-file))
    (teardown)))

(deftest export-types-empty
  (testing "should handle empty schema gracefully"
    (setup)

    (let [json (schema/export-types true)
          parsed (json/parse-string json)]

      (is (contains? parsed "types"))
      (is (vector? (get parsed "types")))
      (is (= 0 (count (get parsed "types")))))
    (teardown)))
