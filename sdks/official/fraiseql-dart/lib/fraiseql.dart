/// FraiseQL Dart SDK — schema authoring and HTTP client.
library fraiseql;

export 'src/client.dart';
export 'src/errors.dart';
// The schema builder the README's Quick Start opens with, and the CRUD generator, were
// both absent from this barrel — `crud_generator.dart` existed but was never exported,
// and the builder did not exist at all, so the documented example failed to compile on
// its first two lines (#853).
//
// `src/authoring/annotations.dart` is gone with them (#1241). Dart has no runtime
// reflection over annotations and this package ships no build_runner generator, so
// nothing read `@FraiseQLType` or `@FraiseQLField` — including their documented
// `crud:` and `computed:` flags. The working authoring surface is the builder below;
// `tools/check-sdk-dead-surface.sh` pins the names so the file cannot come back without
// a consumer, exactly as #926 pinned this SDK's `SqlSourceDispatch` annotation.
export 'src/authoring/schema.dart';
export 'src/authoring/crud_generator.dart';
