# Deprecated

This SDK has been deprecated and is no longer actively maintained.

**Deprecated since**: v2.0.0
**Last compatible schema version**: v1.x
**v2.0.0 compatibility**: Not supported. The v2 compiled schema format (`schema.compiled.json`) is not compatible with this SDK.

## Recommended Alternative

[FraiseQL Java SDK](../fraiseql-java)

## Reason

Clojure runs on the JVM; the Java SDK provides full interop without a separate Clojure SDK.

## Migration

1. Add the Java SDK as a dependency in your `deps.edn` or `project.clj`.
2. Call the Java SDK via Clojure's standard `import` and Java interop syntax.
3. The Java SDK APIs map naturally to Clojure's data-oriented style.
4. If you were using v1.x schemas, recompile with `fraiseql-cli compile` to produce a v2 schema.
