(ns fraiseql.schema-roundtrip-test
  "SDK-3: Schema roundtrip golden test.

  Exercises the full decorator → JSON export pipeline: register a type
  with fields (including a scoped field), export to JSON, and verify that
  the output matches the expected schema.json structure exactly.

  This is the contract between the SDK and the fraiseql-cli compiler.
  If the SDK produces malformed JSON the compiler rejects it — but
  without this test that failure is silent during SDK development."
  (:require [clojure.test :refer [deftest is testing]]
            [fraiseql.schema :as schema]
            [cheshire.core :as json]))

(defn- setup [] (schema/reset))
(defn- teardown [] (schema/reset))

(deftest schema-roundtrip-golden
  (testing "full decorator → export pipeline produces expected schema.json structure"
    (setup)

    ;; Register a realistic type with a mix of field types, including a scoped field.
    (schema/register-type "Article"
      {:id     {:type "ID"      :nullable false}
       :title  {:type "String"  :nullable false}
       :body   {:type "String"  :nullable true}
       :email  {:type "String"  :nullable false :scope "read:Article.email"}}
      "A published article")

    (let [json-str (schema/export-types true)
          parsed   (json/parse-string json-str)]

      ;; Output must be a valid JSON object
      (is (map? parsed) "export must produce a JSON object")

      ;; Must contain exactly the `types` key — no compiler-reserved keys
      (is (contains? parsed "types") "output must have `types` key")
      (is (not (contains? parsed "queries"))    "output must NOT have `queries`")
      (is (not (contains? parsed "mutations"))  "output must NOT have `mutations`")
      (is (not (contains? parsed "observers"))  "output must NOT have `observers`")
      (is (not (contains? parsed "security"))   "output must NOT have `security`")
      (is (not (contains? parsed "federation")) "output must NOT have `federation`")

      ;; Exactly one type was registered
      (let [types (get parsed "types")]
        (is (vector? types) "`types` must be a JSON array")
        (is (= 1 (count types)) "exactly one type was registered"))

      ;; Verify Article type structure
      (let [article (first (get parsed "types"))]
        (is (= "Article" (get article "name"))        "type name must be Article")
        (is (= "A published article" (get article "description")) "description must round-trip")

        ;; All four fields must be present
        (let [fields     (get article "fields")
              field-names (map #(get % "name") fields)]
          (is (some #(= "id" %) field-names)    "field `id` must be present")
          (is (some #(= "title" %) field-names) "field `title` must be present")
          (is (some #(= "body" %) field-names)  "field `body` must be present")
          (is (some #(= "email" %) field-names) "field `email` must be present"))

        ;; The scoped field must carry its scope annotation
        (let [fields     (get article "fields")
              email-field (first (filter #(= "email" (get % "name")) fields))]
          (is (some? email-field) "email field must be present")
          (when email-field
            (is (= "read:Article.email" (get email-field "scope"))
                "scope annotation must round-trip")))))

    (teardown)))

(deftest schema-roundtrip-multiple-types
  (testing "multiple registered types all appear in export with correct names"
    (setup)

    (schema/register-type "User"
      {:id   {:type "ID"     :nullable false}
       :name {:type "String" :nullable false}}
      "System user")

    (schema/register-type "Post"
      {:id    {:type "ID"     :nullable false}
       :title {:type "String" :nullable false}}
      "Blog post")

    (let [json-str (schema/export-types true)
          parsed   (json/parse-string json-str)
          types    (get parsed "types")
          names    (set (map #(get % "name") types))]

      (is (= 2 (count types)) "two types must be exported")
      (is (contains? names "User") "User must be present")
      (is (contains? names "Post") "Post must be present"))

    (teardown)))

(deftest schema-roundtrip-json-is-parseable-as-schema-format
  (testing "exported JSON satisfies the schema.json structural contract"
    (setup)

    (schema/register-type "Order"
      {:id     {:type "ID"      :nullable false}
       :amount {:type "Float"   :nullable false}
       :status {:type "String"  :nullable true}})

    (let [json-str (schema/export-types true)
          parsed   (json/parse-string json-str)]

      ;; Top-level shape: only `types`
      (is (= #{"types"} (set (keys parsed)))
          "schema.json for types-only export must contain exactly the `types` key")

      ;; Each type entry must have at minimum `name` and `fields`
      (doseq [t (get parsed "types")]
        (is (string? (get t "name"))     "every type entry must have a string `name`")
        (is (sequential? (get t "fields")) "every type entry must have a `fields` array")))

    (teardown)))
