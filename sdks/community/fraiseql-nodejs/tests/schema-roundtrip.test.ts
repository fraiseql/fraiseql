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
import { describe, it, expect, beforeEach, afterEach } from '@jest/globals';
import { Schema } from '../src/schema';

describe('Schema Roundtrip — full decorator → export pipeline', () => {
  beforeEach(() => Schema.reset());
  afterEach(() => Schema.reset());

  it('produces the expected schema.json structure for a single type', () => {
    // Register a realistic type with a mix of field types, including a scoped field.
    Schema.registerType('Article', {
      fields: {
        id:    { type: 'ID',     nullable: false },
        title: { type: 'String', nullable: false },
        body:  { type: 'String', nullable: true  },
        email: { type: 'String', nullable: false, scope: 'read:Article.email' },
      },
      description: 'A published article',
    });

    const json = Schema.exportTypes(true);
    const parsed = JSON.parse(json);

    // Output must be a valid JSON object
    expect(typeof parsed).toBe('object');

    // Must contain exactly the `types` key — no compiler-reserved keys
    expect(parsed).toHaveProperty('types');
    expect(parsed).not.toHaveProperty('queries');
    expect(parsed).not.toHaveProperty('mutations');
    expect(parsed).not.toHaveProperty('observers');
    expect(parsed).not.toHaveProperty('security');
    expect(parsed).not.toHaveProperty('federation');

    // Exactly one type was registered
    expect(Array.isArray(parsed.types)).toBe(true);
    expect(parsed.types).toHaveLength(1);

    // Verify Article type structure
    const article = parsed.types[0];
    expect(article.name).toBe('Article');
    expect(article.description).toBe('A published article');

    // All four fields must be present
    const fieldNames: string[] = article.fields.map((f: { name: string }) => f.name);
    expect(fieldNames).toContain('id');
    expect(fieldNames).toContain('title');
    expect(fieldNames).toContain('body');
    expect(fieldNames).toContain('email');

    // The scoped field must carry its scope annotation
    const emailField = article.fields.find((f: { name: string }) => f.name === 'email');
    expect(emailField).toBeDefined();
    expect(emailField.scope).toBe('read:Article.email');
  });

  it('exports multiple types with correct names', () => {
    Schema.registerType('User', {
      fields: {
        id:   { type: 'ID',     nullable: false },
        name: { type: 'String', nullable: false },
      },
      description: 'System user',
    });

    Schema.registerType('Post', {
      fields: {
        id:    { type: 'ID',     nullable: false },
        title: { type: 'String', nullable: false },
      },
      description: 'Blog post',
    });

    const parsed = JSON.parse(Schema.exportTypes(true));
    expect(parsed.types).toHaveLength(2);

    const names: string[] = parsed.types.map((t: { name: string }) => t.name);
    expect(names).toContain('User');
    expect(names).toContain('Post');
  });

  it('exported JSON satisfies the schema.json structural contract', () => {
    Schema.registerType('Order', {
      fields: {
        id:     { type: 'ID',    nullable: false },
        amount: { type: 'Float', nullable: false },
        status: { type: 'String', nullable: true },
      },
    });

    const parsed = JSON.parse(Schema.exportTypes(true));

    // Top-level shape: only `types`
    expect(Object.keys(parsed)).toEqual(['types']);

    // Each type entry must have at minimum `name` and `fields`
    for (const t of parsed.types) {
      expect(typeof t.name).toBe('string');
      expect(Array.isArray(t.fields)).toBe(true);
    }
  });
});
