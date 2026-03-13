# Deprecated

This SDK has been deprecated and is no longer actively maintained.

**Deprecated since**: v2.0.0
**Last compatible schema version**: v1.x
**v2.0.0 compatibility**: Not supported. The v2 compiled schema format (`schema.compiled.json`) is not compatible with this SDK.

## Recommended Alternative

[FraiseQL Java SDK](../fraiseql-java)

## Reason

Scala runs on the JVM; the Java SDK provides full interop without a separate Scala SDK.

## Migration

1. Add the Java SDK dependency to your `build.sbt` or `pom.xml`.
2. Replace `fraiseql.scala.*` imports with `io.fraiseql.*`.
3. The Java SDK is fully callable from Scala with idiomatic interop.
4. If you were using v1.x schemas, recompile with `fraiseql-cli compile` to produce a v2 schema.
