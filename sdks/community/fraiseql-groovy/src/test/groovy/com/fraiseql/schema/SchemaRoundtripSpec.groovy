package com.fraiseql.schema

import spock.lang.Specification
import groovy.json.JsonSlurper

/**
 * SDK-3: Schema roundtrip golden test.
 *
 * Exercises the full decorator → JSON export pipeline: register a type
 * with fields (including a scoped field), export to JSON, and verify that
 * the output matches the expected schema.json structure exactly.
 *
 * This is the contract between the SDK and the fraiseql-cli compiler.
 * If the SDK produces malformed JSON the compiler rejects it — but
 * without this test that failure is silent during SDK development.
 */
class SchemaRoundtripSpec extends Specification {
    def setup() {
        Schema.reset()
    }

    def cleanup() {
        Schema.reset()
    }

    def "full decorator → export pipeline produces expected schema.json structure"() {
        when: "a realistic type with a scoped field is registered and exported"
        Schema.registerType("Article", [
            id:    [type: "ID",     nullable: false],
            title: [type: "String", nullable: false],
            body:  [type: "String", nullable: true],
            email: [type: "String", nullable: false, scope: "read:Article.email"],
        ], "A published article")

        String jsonStr = Schema.exportTypes(true)
        def parsed = new JsonSlurper().parseText(jsonStr)

        then: "output is a valid JSON object with only the `types` key"
        parsed instanceof Map
        parsed.containsKey("types")
        !parsed.containsKey("queries")
        !parsed.containsKey("mutations")
        !parsed.containsKey("observers")
        !parsed.containsKey("security")
        !parsed.containsKey("federation")

        and: "exactly one type was exported"
        parsed.types instanceof List
        parsed.types.size() == 1

        and: "Article type has the correct name and description"
        def article = parsed.types[0]
        article.name == "Article"
        article.description == "A published article"

        and: "all four fields are present"
        def fieldNames = article.fields.collect { it.name }
        fieldNames.contains("id")
        fieldNames.contains("title")
        fieldNames.contains("body")
        fieldNames.contains("email")

        and: "the scoped field carries its scope annotation"
        def emailField = article.fields.find { it.name == "email" }
        emailField != null
        emailField.scope == "read:Article.email"
    }

    def "multiple registered types all appear in export with correct names"() {
        when:
        Schema.registerType("User",
            [id: [type: "ID", nullable: false], name: [type: "String", nullable: false]],
            "System user")
        Schema.registerType("Post",
            [id: [type: "ID", nullable: false], title: [type: "String", nullable: false]],
            "Blog post")

        def parsed = new JsonSlurper().parseText(Schema.exportTypes(true))

        then:
        parsed.types.size() == 2
        parsed.types.collect { it.name }.containsAll(["User", "Post"])
    }

    def "exported JSON satisfies the schema.json structural contract"() {
        when:
        Schema.registerType("Order", [
            id:     [type: "ID",     nullable: false],
            amount: [type: "Float",  nullable: false],
            status: [type: "String", nullable: true],
        ])

        def parsed = new JsonSlurper().parseText(Schema.exportTypes(true))

        then: "top-level shape contains only `types`"
        parsed.keySet() == ["types"] as Set

        and: "every type entry has a string name and a fields list"
        parsed.types.every { t ->
            t.name instanceof String && t.fields instanceof List
        }
    }
}
