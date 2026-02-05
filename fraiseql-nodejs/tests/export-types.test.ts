import { describe, it, expect, beforeEach, afterEach } from '@jest/globals';
import { Schema } from '../src/schema';
import * as fs from 'fs';
import * as path from 'path';

/**
 * Tests for minimal types.json export (TOML-based workflow)
 *
 * Validates that exportTypes() function generates minimal schema
 * with only types (no queries, mutations, observers, security, etc.)
 */
describe('Export Types - Minimal Schema Export', () => {
  beforeEach(() => {
    // Reset schema registry before each test
    Schema.reset();
  });

  afterEach(() => {
    // Clean up after tests
    Schema.reset();
  });

  it('should export minimal schema with single type', () => {
    // Register a single type
    Schema.registerType('User', {
      fields: {
        id: { type: 'ID', nullable: false },
        name: { type: 'String', nullable: false },
        email: { type: 'String', nullable: false },
      },
      description: 'User in the system',
    });

    // Export minimal types
    const json = Schema.exportTypes(true);
    const parsed = JSON.parse(json);

    // Should have types section
    expect(parsed).toHaveProperty('types');
    expect(Array.isArray(parsed.types)).toBe(true);
    expect(parsed.types).toHaveLength(1);

    // Should NOT have queries, mutations, observers
    expect(parsed).not.toHaveProperty('queries');
    expect(parsed).not.toHaveProperty('mutations');
    expect(parsed).not.toHaveProperty('observers');
    expect(parsed).not.toHaveProperty('authz_policies');

    // Verify User type
    const userDef = parsed.types[0];
    expect(userDef.name).toBe('User');
    expect(userDef.description).toBe('User in the system');
  });

  it('should export minimal schema with multiple types', () => {
    // Register User type
    Schema.registerType('User', {
      fields: {
        id: { type: 'ID', nullable: false },
        name: { type: 'String', nullable: false },
      },
    });

    // Register Post type
    Schema.registerType('Post', {
      fields: {
        id: { type: 'ID', nullable: false },
        title: { type: 'String', nullable: false },
        authorId: { type: 'ID', nullable: false },
      },
    });

    // Export minimal
    const json = Schema.exportTypes(true);
    const parsed = JSON.parse(json);

    // Check types count
    expect(parsed.types).toHaveLength(2);

    // Verify both types present
    const typeNames = parsed.types.map((t: any) => t.name);
    expect(typeNames).toContain('User');
    expect(typeNames).toContain('Post');
  });

  it('should not include queries in minimal export', () => {
    // Register type
    Schema.registerType('User', {
      fields: {
        id: { type: 'ID', nullable: false },
      },
    });

    // Export minimal
    const json = Schema.exportTypes(true);
    const parsed = JSON.parse(json);

    // Should have types
    expect(parsed).toHaveProperty('types');

    // Should NOT have queries
    expect(parsed).not.toHaveProperty('queries');
    expect(parsed).not.toHaveProperty('mutations');
  });

  it('should export compact format when pretty is false', () => {
    Schema.registerType('User', {
      fields: {
        id: { type: 'ID', nullable: false },
      },
    });

    // Export compact
    const compact = Schema.exportTypes(false);

    // Should be valid JSON
    const parsed = JSON.parse(compact);
    expect(parsed).toHaveProperty('types');

    // Compact JSON should be smaller than pretty-printed
    const pretty = Schema.exportTypes(true);
    expect(compact.length).toBeLessThan(pretty.length);
  });

  it('should export pretty format when pretty is true', () => {
    Schema.registerType('User', {
      fields: {
        id: { type: 'ID', nullable: false },
      },
    });

    // Export pretty
    const json = Schema.exportTypes(true);

    // Should contain newlines (pretty format)
    expect(json).toContain('\n');

    // Should be valid JSON
    const parsed = JSON.parse(json);
    expect(parsed).toHaveProperty('types');
  });

  it('should export types to file', () => {
    Schema.registerType('User', {
      fields: {
        id: { type: 'ID', nullable: false },
        name: { type: 'String', nullable: false },
      },
    });

    // Export to temporary file
    const tmpFile = '/tmp/fraiseql_types_test_nodejs.json';

    // Remove file if exists
    if (fs.existsSync(tmpFile)) {
      fs.unlinkSync(tmpFile);
    }

    // Export to file
    Schema.exportTypesFile(tmpFile);

    // Verify file exists and is valid JSON
    expect(fs.existsSync(tmpFile)).toBe(true);

    const content = fs.readFileSync(tmpFile, 'utf-8');
    const parsed = JSON.parse(content);

    expect(parsed).toHaveProperty('types');
    expect(parsed.types).toHaveLength(1);

    // Cleanup
    fs.unlinkSync(tmpFile);
  });

  it('should handle empty schema gracefully', () => {
    // Export with no types registered
    const json = Schema.exportTypes(true);
    const parsed = JSON.parse(json);

    // Should still have types key (as empty array)
    expect(parsed).toHaveProperty('types');
    expect(Array.isArray(parsed.types)).toBe(true);
    expect(parsed.types).toHaveLength(0);
  });
});
