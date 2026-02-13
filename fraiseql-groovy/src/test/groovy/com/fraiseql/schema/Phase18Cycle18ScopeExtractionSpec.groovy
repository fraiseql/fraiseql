package com.fraiseql.schema

import groovy.json.JsonSlurper
import spock.lang.Specification
import spock.lang.BeforeEach

/**
 * Phase 18 Cycle 18: Groovy SDK - Field Scope Extraction & Validation
 *
 * RED phase tests for field-level RBAC scope extraction.
 * Tests cover:
 * - Field creation with scope metadata
 * - Single scope requirements (scope key)
 * - Multiple scopes array (scopes key)
 * - Scope pattern validation (action:resource format)
 * - SchemaRegistry for type tracking
 * - JSON export with scope metadata
 */
class Phase18Cycle18ScopeExtractionSpec extends Specification {

  @BeforeEach
  void setup() {
    Schema.reset()
  }

  // ============================================================================
  // FIELD CREATION TESTS (3 tests)
  // ============================================================================

  def "Field should create with all properties"() {
    when:
    Map<String, Map<String, Object>> fields = [
      email: [
        type: 'String',
        nullable: false,
        description: 'User email address',
        scope: 'read:user.email'
      ]
    ]
    Schema.registerType('User', fields)

    then:
    Schema.getTypeNames().contains('User')
    def field = Schema.getType('User').fields.email
    field.type == 'String'
    field.nullable == false
    field.description == 'User email address'
    field.scope == 'read:user.email'
  }

  def "Field should create with minimal properties"() {
    when:
    Map<String, Map<String, Object>> fields = [
      id: [type: 'Int']
    ]
    Schema.registerType('User', fields)

    then:
    def field = Schema.getType('User').fields.id
    field.type == 'Int'
    field.nullable == false
    field.scope == null
    field.scopes == null
  }

  def "Field should preserve metadata alongside scopes"() {
    when:
    Map<String, Map<String, Object>> fields = [
      password: [
        type: 'String',
        nullable: false,
        description: 'Hashed password',
        scope: 'admin:user.*'
      ]
    ]
    Schema.registerType('User', fields)

    then:
    def field = Schema.getType('User').fields.password
    field.type == 'String'
    field.scope == 'admin:user.*'
    field.description == 'Hashed password'
  }

  // ============================================================================
  // SINGLE SCOPE REQUIREMENT TESTS (3 tests)
  // ============================================================================

  def "Field should support single scope format"() {
    when:
    Map<String, Map<String, Object>> fields = [
      email: [type: 'String', scope: 'read:user.email']
    ]
    Schema.registerType('User', fields)

    then:
    def field = Schema.getType('User').fields.email
    field.scope == 'read:user.email'
    field.scopes == null
  }

  def "Field should support wildcard resource scope"() {
    when:
    Map<String, Map<String, Object>> fields = [
      profile: [type: 'Object', scope: 'read:User.*']
    ]
    Schema.registerType('User', fields)

    then:
    Schema.getType('User').fields.profile.scope == 'read:User.*'
  }

  def "Field should support global wildcard scope"() {
    when:
    Map<String, Map<String, Object>> fields = [
      secret: [type: 'String', scope: 'admin:*']
    ]
    Schema.registerType('User', fields)

    then:
    Schema.getType('User').fields.secret.scope == 'admin:*'
  }

  // ============================================================================
  // MULTIPLE SCOPES ARRAY TESTS (3 tests)
  // ============================================================================

  def "Field should support multiple scopes array"() {
    when:
    Map<String, Map<String, Object>> fields = [
      email: [type: 'String', scopes: ['read:user.email', 'write:user.email']]
    ]
    Schema.registerType('User', fields)

    then:
    def field = Schema.getType('User').fields.email
    field.scopes == ['read:user.email', 'write:user.email']
    field.scope == null
  }

  def "Field should support single element scopes array"() {
    when:
    Map<String, Map<String, Object>> fields = [
      profile: [type: 'Object', scopes: ['read:user.profile']]
    ]
    Schema.registerType('User', fields)

    then:
    def field = Schema.getType('User').fields.profile
    field.scopes == ['read:user.profile']
    field.scopes.size() == 1
  }

  def "Field should support complex scopes array"() {
    when:
    Map<String, Map<String, Object>> fields = [
      data: [type: 'String', scopes: ['read:user.email', 'write:user.*', 'admin:*']]
    ]
    Schema.registerType('User', fields)

    then:
    def field = Schema.getType('User').fields.data
    field.scopes.size() == 3
    field.scopes.contains('read:user.email')
    field.scopes.contains('write:user.*')
    field.scopes.contains('admin:*')
  }

  // ============================================================================
  // SCOPE PATTERN VALIDATION TESTS (6 tests)
  // ============================================================================

  def "ScopeValidator should validate specific field scope"() {
    expect:
    ScopeValidator.validate('read:user.email') == true
  }

  def "ScopeValidator should validate resource wildcard scope"() {
    expect:
    ScopeValidator.validate('read:User.*') == true
  }

  def "ScopeValidator should validate global admin wildcard"() {
    expect:
    ScopeValidator.validate('admin:*') == true
  }

  def "ScopeValidator should reject scope missing colon"() {
    expect:
    ScopeValidator.validate('readuser') == false
  }

  def "ScopeValidator should reject action with hyphen"() {
    expect:
    ScopeValidator.validate('read-all:user') == false
  }

  def "ScopeValidator should reject resource with hyphen"() {
    expect:
    ScopeValidator.validate('read:user-data') == false
  }

  // ============================================================================
  // SCHEMA REGISTRY TESTS (3 tests)
  // ============================================================================

  def "Schema should register type with fields and scopes"() {
    when:
    Map<String, Map<String, Object>> fields = [
      id: [type: 'Int', nullable: false],
      email: [type: 'String', nullable: false, scope: 'read:user.email']
    ]
    Schema.registerType('User', fields)

    then:
    Schema.getTypeNames().contains('User')
  }

  def "Schema should extract scoped fields from registry"() {
    when:
    Map<String, Map<String, Object>> fields = [
      id: [type: 'Int', nullable: false],
      email: [type: 'String', nullable: false, scope: 'read:user.email'],
      password: [type: 'String', nullable: false, scope: 'admin:user.password']
    ]
    Schema.registerType('User', fields)

    then:
    Schema.getTypeNames().contains('User')
    // Verify scoped fields exist
    Schema.getType('User').fields.email.scope == 'read:user.email'
    Schema.getType('User').fields.password.scope == 'admin:user.password'
  }

  def "Schema should handle multiple types with different scopes"() {
    when:
    Schema.registerType('User', [
      id: [type: 'Int'],
      email: [type: 'String', scope: 'read:user.email']
    ])
    Schema.registerType('Post', [
      id: [type: 'Int'],
      content: [type: 'String', scope: 'read:post.content']
    ])

    then:
    Schema.getTypeNames().size() == 2
    Schema.getTypeNames().contains('User')
    Schema.getTypeNames().contains('Post')
  }

  // ============================================================================
  // JSON EXPORT TESTS (2 tests)
  // ============================================================================

  def "Schema export should include scope in field JSON"() {
    when:
    Schema.registerType('User', [
      email: [type: 'String', nullable: false, scope: 'read:user.email']
    ])
    String json = Schema.exportTypes(false)
    def parsed = new JsonSlurper().parseText(json)

    then:
    def types = parsed.types
    types.size() == 1
    types[0].name == 'User'
    def field = types[0].fields[0]
    field.name == 'email'
    field.type == 'String'
    field.scope == 'read:user.email'
  }

  def "Schema export should export multiple types with scopes"() {
    when:
    Schema.registerType('User', [
      id: [type: 'Int'],
      email: [type: 'String', scope: 'read:user.email']
    ])
    Schema.registerType('Post', [
      id: [type: 'Int'],
      content: [type: 'String', scope: 'read:post.content']
    ])
    String json = Schema.exportTypes(false)
    def parsed = new JsonSlurper().parseText(json)

    then:
    def types = parsed.types
    types.size() == 2
    // Find User and Post
    types.any { it.name == 'User' }
    types.any { it.name == 'Post' }
    // Check for scope fields
    json.contains('read:user.email')
    json.contains('read:post.content')
  }

  // ============================================================================
  // CONFLICTING SCOPE AND SCOPES TESTS (2 tests)
  // ============================================================================

  def "Field with both scope and scopes should preserve both"() {
    when:
    Map<String, Map<String, Object>> fields = [
      email: [
        type: 'String',
        scope: 'read:user.email',
        scopes: ['write:user.email']
      ]
    ]
    Schema.registerType('User', fields)

    then:
    def field = Schema.getType('User').fields.email
    field.scope == 'read:user.email'
    field.scopes == ['write:user.email']
  }

  def "ScopeValidator should reject empty scope string"() {
    expect:
    ScopeValidator.validate('') == false
  }
}
