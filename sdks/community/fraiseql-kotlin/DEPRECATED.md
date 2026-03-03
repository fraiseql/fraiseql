# Deprecated

This SDK has been deprecated and is no longer actively maintained.

**Deprecated since**: v2.0.0
**Last compatible schema version**: v1.x
**v2.0.0 compatibility**: Not supported. The v2 compiled schema format (`schema.compiled.json`) is not compatible with this SDK.

## Recommended Alternative

[FraiseQL Java SDK](../fraiseql-java)

## Reason

Kotlin runs on the JVM; the Java SDK provides first-class interop without a separate Kotlin SDK.

## Migration

1. Add the Java SDK dependency to your `build.gradle.kts` or `pom.xml`.
2. Replace `fraiseql-kotlin` imports with `io.fraiseql.*`.
3. The Java SDK is fully Kotlin-compatible — all APIs are callable from Kotlin without any adapters.
4. If you were using v1.x schemas, recompile with `fraiseql-cli compile` to produce a v2 schema.
