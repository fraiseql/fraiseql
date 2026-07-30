/// FraiseQL Dart SDK — schema authoring and HTTP client.
library fraiseql;

export 'src/client.dart';
export 'src/errors.dart';
export 'src/authoring/annotations.dart';
// The schema builder the README's Quick Start opens with, and the CRUD generator, were
// both absent from this barrel — `crud_generator.dart` existed but was never exported,
// and the builder did not exist at all, so the documented example failed to compile on
// its first two lines (#853).
export 'src/authoring/schema.dart';
export 'src/authoring/crud_generator.dart';
