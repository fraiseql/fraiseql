package com.fraiseql.schema

import spock.lang.Specification
import groovy.json.JsonSlurper
import java.nio.file.Files
import java.nio.file.Paths

class SchemaSpec extends Specification {
  def setup() {
    Schema.reset()
  }

  def cleanup() {
    Schema.reset()
  }

  def "exports minimal schema with single type"() {
    when:
    Schema.registerType("User", [
      id: [type: "ID", nullable: false],
      name: [type: "String", nullable: false],
      email: [type: "String", nullable: false]
    ], "User in the system")

    String json = Schema.exportTypes(true)
    def parsed = new JsonSlurper().parseText(json)

    then:
    parsed.types != null
    parsed.types.size() == 1
    !parsed.containsKey("queries")
    !parsed.containsKey("mutations")
    !parsed.containsKey("observers")

    def userDef = parsed.types[0]
    userDef.name == "User"
    userDef.description == "User in the system"
  }

  def "exports minimal schema with multiple types"() {
    when:
    Schema.registerType("User", [
      id: [type: "ID", nullable: false],
      name: [type: "String", nullable: false]
    ])

    Schema.registerType("Post", [
      id: [type: "ID", nullable: false],
      title: [type: "String", nullable: false]
    ])

    String json = Schema.exportTypes(true)
    def parsed = new JsonSlurper().parseText(json)

    then:
    parsed.types.size() == 2
    parsed.types.collect { it.name }.containsAll(["User", "Post"])
  }

  def "does not include queries in minimal export"() {
    when:
    Schema.registerType("User", [
      id: [type: "ID", nullable: false]
    ])

    String json = Schema.exportTypes(true)
    def parsed = new JsonSlurper().parseText(json)

    then:
    parsed.types != null
    !parsed.containsKey("queries")
    !parsed.containsKey("mutations")
  }

  def "exports compact format when pretty is false"() {
    when:
    Schema.registerType("User", [
      id: [type: "ID", nullable: false]
    ])

    String compact = Schema.exportTypes(false)
    String pretty = Schema.exportTypes(true)

    then:
    compact.length() <= pretty.length()
    new JsonSlurper().parseText(compact).types != null
  }

  def "exports pretty format when pretty is true"() {
    when:
    Schema.registerType("User", [
      id: [type: "ID", nullable: false]
    ])

    String json = Schema.exportTypes(true)

    then:
    json.contains("\n")
    new JsonSlurper().parseText(json).types != null
  }

  def "exports types to file"() {
    when:
    Schema.registerType("User", [
      id: [type: "ID", nullable: false],
      name: [type: "String", nullable: false]
    ])

    String tmpFile = "/tmp/fraiseql_types_test_groovy.json"
    File f = new File(tmpFile)
    if (f.exists()) f.delete()

    Schema.exportTypesFile(tmpFile)

    then:
    f.exists()

    when:
    String content = new File(tmpFile).text
    def parsed = new JsonSlurper().parseText(content)

    then:
    parsed.types != null
    parsed.types.size() == 1

    cleanup:
    new File(tmpFile).delete()
  }

  def "handles empty schema gracefully"() {
    when:
    String json = Schema.exportTypes(true)
    def parsed = new JsonSlurper().parseText(json)

    then:
    parsed.types != null
    parsed.types.isEmpty()
  }
}
