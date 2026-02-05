import { describe, it, expect, beforeEach } from '@jest/globals';
import { Schema } from '../src/schema';

/**
 * Phase 18 Cycle 14: Field-Level RBAC for Node.js SDK
 *
 * Tests that field scopes are properly extracted from field configuration,
 * stored in field registry, and exported to JSON for compiler consumption.
 *
 * RED Phase: 21 comprehensive test cases
 * - 15 happy path tests for scope extraction and export
 * - 6 validation tests for error handling
 *
 * Field format:
 * - Single scope: { type: 'Float', requiresScope: 'read:user.salary' }
 * - Multiple scopes: { type: 'String', requiresScopes: ['admin', 'auditor'] }
 */

describe('Phase 18 Cycle 14: Node.js SDK Field Scope Extraction & Export', () => {
  beforeEach(() => {
    Schema.reset();
  });

  // =========================================================================
  // HAPPY PATH: SINGLE SCOPE EXTRACTION (3 tests)
  // =========================================================================

  describe('Single scope extraction', () => {
    it('extracts single scope from field configuration', () => {
      // RED: This test fails because FieldDefinition doesn't store scope
      Schema.registerType('UserWithScope', {
        fields: {
          id: { type: 'Int' },
          salary: { type: 'Float', requiresScope: 'read:user.salary' },
        },
      });

      const typeInfo = (Schema as any).getTypeRegistry().getType('UserWithScope');
      expect(typeInfo).toBeDefined();
      expect(Object.keys(typeInfo.fields)).toHaveLength(2);

      const salaryField = typeInfo.fields.salary;
      expect(salaryField).toBeDefined();
      expect(salaryField.requiresScope).toBe('read:user.salary');
    });

    it('extracts multiple different scopes on different fields', () => {
      // RED: Tests extraction of different scopes on different fields
      Schema.registerType('UserWithMultipleScopes', {
        fields: {
          id: { type: 'Int' },
          email: { type: 'String', requiresScope: 'read:user.email' },
          phone: { type: 'String', requiresScope: 'read:user.phone' },
          ssn: { type: 'String', requiresScope: 'read:user.ssn' },
        },
      });

      const typeInfo = (Schema as any).getTypeRegistry().getType('UserWithMultipleScopes');
      expect(typeInfo).toBeDefined();

      expect(typeInfo.fields.email.requiresScope).toBe('read:user.email');
      expect(typeInfo.fields.phone.requiresScope).toBe('read:user.phone');
      expect(typeInfo.fields.ssn.requiresScope).toBe('read:user.ssn');
    });

    it('handles public fields without scope requirement', () => {
      // RED: Public fields should have undefined/no scope
      Schema.registerType('UserWithMixedFields', {
        fields: {
          id: { type: 'Int' },
          name: { type: 'String' },
          email: { type: 'String', requiresScope: 'read:user.email' },
        },
      });

      const typeInfo = (Schema as any).getTypeRegistry().getType('UserWithMixedFields');
      const idField = typeInfo.fields.id;
      expect(idField.requiresScope).toBeUndefined();
    });
  });

  // =========================================================================
  // HAPPY PATH: MULTIPLE SCOPES ON SINGLE FIELD (3 tests)
  // =========================================================================

  describe('Multiple scopes on single field', () => {
    it('extracts multiple scopes on single field as array', () => {
      // RED: Field with requiresScopes array
      Schema.registerType('AdminWithMultipleScopes', {
        fields: {
          id: { type: 'Int' },
          adminNotes: { type: 'String', requiresScopes: ['admin', 'auditor'] },
        },
      });

      const typeInfo = (Schema as any).getTypeRegistry().getType('AdminWithMultipleScopes');
      const adminField = typeInfo.fields.adminNotes;

      expect(adminField).toBeDefined();
      expect(adminField.requiresScopes).toBeDefined();
      expect(adminField.requiresScopes).toHaveLength(2);
      expect(adminField.requiresScopes).toContain('admin');
      expect(adminField.requiresScopes).toContain('auditor');
    });

    it('mixes single-scope and multi-scope fields', () => {
      // RED: Type with both single-scope and multi-scope fields
      Schema.registerType('MixedScopeTypes', {
        fields: {
          basicField: { type: 'String', requiresScope: 'read:basic' },
          advancedField: { type: 'String', requiresScopes: ['read:advanced', 'admin'] },
        },
      });

      const typeInfo = (Schema as any).getTypeRegistry().getType('MixedScopeTypes');

      expect(typeInfo.fields.basicField.requiresScope).toBe('read:basic');
      expect(typeInfo.fields.advancedField.requiresScopes).toHaveLength(2);
    });

    it('preserves scope array order', () => {
      // RED: Scopes array order must be preserved
      Schema.registerType('OrderedScopes', {
        fields: {
          restricted: { type: 'String', requiresScopes: ['first', 'second', 'third'] },
        },
      });

      const typeInfo = (Schema as any).getTypeRegistry().getType('OrderedScopes');
      const scopes = typeInfo.fields.restricted.requiresScopes;

      expect(scopes).toHaveLength(3);
      expect(scopes[0]).toBe('first');
      expect(scopes[1]).toBe('second');
      expect(scopes[2]).toBe('third');
    });
  });

  // =========================================================================
  // HAPPY PATH: SCOPE PATTERNS (3 tests)
  // =========================================================================

  describe('Scope patterns', () => {
    it('supports resource-based scope pattern', () => {
      // RED: Resource pattern like read:User.email
      Schema.registerType('ResourcePatternScopes', {
        fields: {
          email: { type: 'String', requiresScope: 'read:User.email' },
          phone: { type: 'String', requiresScope: 'read:User.phone' },
        },
      });

      const typeInfo = (Schema as any).getTypeRegistry().getType('ResourcePatternScopes');

      expect(typeInfo.fields.email.requiresScope).toBe('read:User.email');
    });

    it('supports action-based scope pattern', () => {
      // RED: Action patterns like read:*, write:*, admin:*
      Schema.registerType('ActionPatternScopes', {
        fields: {
          readableField: { type: 'String', requiresScope: 'read:User.*' },
          writableField: { type: 'String', requiresScope: 'write:User.*' },
        },
      });

      const typeInfo = (Schema as any).getTypeRegistry().getType('ActionPatternScopes');

      expect(typeInfo.fields.readableField.requiresScope).toBe('read:User.*');
      expect(typeInfo.fields.writableField.requiresScope).toBe('write:User.*');
    });

    it('supports global wildcard scope', () => {
      // RED: Global wildcard matching all scopes
      Schema.registerType('GlobalWildcardScope', {
        fields: {
          adminOverride: { type: 'String', requiresScope: '*' },
        },
      });

      const typeInfo = (Schema as any).getTypeRegistry().getType('GlobalWildcardScope');

      expect(typeInfo.fields.adminOverride.requiresScope).toBe('*');
    });
  });

  // =========================================================================
  // HAPPY PATH: JSON EXPORT (3 tests)
  // =========================================================================

  describe('JSON export of scopes', () => {
    it('exports single scope to JSON', () => {
      // RED: Scope must appear in JSON export
      Schema.registerType('ExportTestSingleScope', {
        fields: {
          salary: { type: 'Float', requiresScope: 'read:user.salary' },
        },
      });

      const json = Schema.exportTypes(true);
      const schema = JSON.parse(json);

      expect(schema).toHaveProperty('types');
      expect(schema.types).toHaveLength(1);

      const salaryField = schema.types[0].fields[0];
      expect(salaryField).toHaveProperty('requiresScope');
      expect(salaryField.requiresScope).toBe('read:user.salary');
    });

    it('exports multiple scopes array to JSON', () => {
      // RED: requiresScopes array exported correctly
      Schema.registerType('ExportTestMultipleScopes', {
        fields: {
          restricted: { type: 'String', requiresScopes: ['scope1', 'scope2'] },
        },
      });

      const json = Schema.exportTypes(true);
      const schema = JSON.parse(json);

      const field = schema.types[0].fields[0];
      expect(field).toHaveProperty('requiresScopes');
      expect(Array.isArray(field.requiresScopes)).toBe(true);
      expect(field.requiresScopes).toHaveLength(2);
    });

    it('omits scope fields for public fields in JSON', () => {
      // RED: Public fields should NOT have scope in JSON
      Schema.registerType('ExportTestPublicField', {
        fields: {
          id: { type: 'Int' },
          name: { type: 'String' },
        },
      });

      const json = Schema.exportTypes(true);
      const schema = JSON.parse(json);

      const idField = schema.types[0].fields[0];
      expect(idField).not.toHaveProperty('requiresScope');
      expect(idField).not.toHaveProperty('requiresScopes');
    });
  });

  // =========================================================================
  // HAPPY PATH: SCOPE WITH OTHER METADATA (3 tests)
  // =========================================================================

  describe('Scope with other field metadata', () => {
    it('preserves scope alongside other field metadata', () => {
      // RED: Scope doesn't interfere with type, nullable, description
      Schema.registerType('ScopeWithMetadata', {
        fields: {
          salary: {
            type: 'Float',
            requiresScope: 'read:user.salary',
            description: "User's annual salary",
            nullable: false,
          },
        },
      });

      const typeInfo = (Schema as any).getTypeRegistry().getType('ScopeWithMetadata');
      const salaryField = typeInfo.fields.salary;

      expect(salaryField.type).toBe('Float');
      expect(salaryField.requiresScope).toBe('read:user.salary');
      expect(salaryField.description).toBe("User's annual salary");
      expect(salaryField.nullable).toBe(false);
    });

    it('works with nullable fields', () => {
      // RED: Scope works on nullable fields
      Schema.registerType('ScopeWithNullable', {
        fields: {
          optionalEmail: {
            type: 'String',
            nullable: true,
            requiresScope: 'read:user.email',
          },
        },
      });

      const typeInfo = (Schema as any).getTypeRegistry().getType('ScopeWithNullable');
      const emailField = typeInfo.fields.optionalEmail;

      expect(emailField.nullable).toBe(true);
      expect(emailField.requiresScope).toBe('read:user.email');
    });

    it('maintains metadata independence across multiple scoped fields', () => {
      // RED: Each field's metadata is independent
      Schema.registerType('MetadataIndependence', {
        fields: {
          field1: {
            type: 'String',
            requiresScope: 'scope1',
            description: 'Desc 1',
          },
          field2: {
            type: 'String',
            requiresScope: 'scope2',
            description: 'Desc 2',
          },
        },
      });

      const typeInfo = (Schema as any).getTypeRegistry().getType('MetadataIndependence');
      const fields = typeInfo.fields;

      expect(fields.field1.requiresScope).toBe('scope1');
      expect(fields.field1.description).toBe('Desc 1');
      expect(fields.field2.requiresScope).toBe('scope2');
      expect(fields.field2.description).toBe('Desc 2');
    });
  });

  // =========================================================================
  // VALIDATION: ERROR HANDLING (6 tests)
  // =========================================================================

  describe('Scope validation and error handling', () => {
    it('detects invalid scope format', () => {
      // RED: Invalid scopes should raise error
      expect(() => {
        Schema.registerType('InvalidScopeFormat', {
          fields: {
            field: { type: 'String', requiresScope: 'invalid_scope_no_colon' },
          },
        });
      }).toThrow();
    });

    it('rejects empty scope string', () => {
      // RED: Empty string scope invalid
      expect(() => {
        Schema.registerType('EmptyScope', {
          fields: {
            field: { type: 'String', requiresScope: '' },
          },
        });
      }).toThrow();
    });

    it('rejects empty scopes array', () => {
      // RED: Empty array not allowed
      expect(() => {
        Schema.registerType('EmptyScopesArray', {
          fields: {
            field: { type: 'String', requiresScopes: [] },
          },
        });
      }).toThrow();
    });

    it('catches invalid action with hyphens', () => {
      // RED: Hyphens in action prefix invalid
      expect(() => {
        Schema.registerType('InvalidActionWithHyphens', {
          fields: {
            field: { type: 'String', requiresScope: 'invalid-action:resource' },
          },
        });
      }).toThrow();
    });

    it('catches invalid resource with hyphens', () => {
      // RED: Hyphens in resource name invalid
      expect(() => {
        Schema.registerType('InvalidResourceWithHyphens', {
          fields: {
            field: { type: 'String', requiresScope: 'read:invalid-resource-name' },
          },
        });
      }).toThrow();
    });

    it('rejects conflicting both scope and scopes', () => {
      // RED: Can't have both on same field
      expect(() => {
        Schema.registerType('ConflictingScopeAndScopes', {
          fields: {
            field: {
              type: 'String',
              requiresScope: 'read:user.email',
              requiresScopes: ['admin', 'auditor'],
            },
          },
        });
      }).toThrow();
    });
  });
});
