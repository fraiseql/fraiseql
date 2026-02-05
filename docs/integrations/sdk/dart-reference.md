# FraiseQL Dart SDK Reference

**Status**: Production-Ready | **Dart Version**: 3.0+ | **Flutter**: 3.0+ | **Null Safety**: Full Support
**Last Updated**: 2026-02-05 | **Maintained By**: FraiseQL Community

Complete API reference for the FraiseQL Dart SDK. Build type-safe GraphQL APIs using Dart 3.0's sound null safety and Flutter integration. Full support for async/await, JSON serialization with `json_serializable`, and seamless Flutter widget integration with Provider, Riverpod, and GetX state management.

## Installation

### Pub.dev Setup

Add FraiseQL to your `pubspec.yaml`:

```yaml
# pubspec.yaml
name: my_fraiseql_app
description: A FraiseQL client application with Flutter.

environment:
  SDK: '>=3.0.0 <4.0.0'
  flutter: '>=3.0.0'

dependencies:
  flutter:
    SDK: flutter
  FraiseQL: ^2.0.0
  fraiseql_flutter: ^2.0.0  # Optional: Flutter widgets

dev_dependencies:
  flutter_test:
    SDK: flutter
  build_runner: ^2.4.0
  json_serializable: ^6.7.0
  riverpod_generator: ^2.3.0  # Optional: State management
```text

Then install dependencies:

```bash
flutter pub get
# or
dart pub get
```text

**Requirements**:

- Dart SDK 3.0 or later (full null safety)
- Flutter 3.0+ (for Flutter features)
- Pub.dev package management

**Supported Platforms**:

- iOS (primary)
- Android (primary)
- Web (Dart with Flutter for Web)
- macOS (desktop)
- Windows (desktop)
- Linux (desktop)

## Quick Reference Table

| Feature | Attribute | Purpose | Signature |
|---------|-----------|---------|-----------|
| **Types** | `@Type()` | Define GraphQL object types | `class User with Serializable` |
| **Queries** | `@Query()` | Read operations (SELECT) | `Future<T> getUser(...)` |
| **Mutations** | `@Mutation()` | Write operations (INSERT/UPDATE/DELETE) | `Future<T> createUser(...)` |
| **Subscriptions** | `@Subscription()` | Real-time event streams (WebSocket) | `Stream<T> onUserUpdate(...)` |
| **Fact Tables** | `@FactTable()` | Analytics OLAP tables | `class UserMetrics` |
| **Aggregate Queries** | `@AggregateQuery()` | GROUP BY aggregations | `Future<List<Aggregate>>` |
| **Field Metadata** | `@Field()` | Field-level config (validation, security) | `@Field(required: true)` |
| **RBAC** | `@requiresScope()` | Field-level access control | `@requiresScope(['admin'])` |
| **JSON Serialization** | `@JsonSerializable()` | JSON encode/decode | `User.fromJson()` |
| **Schema Export** | `exportSchema()` | Generate schema.json | `Dart code generation` |

## Type System with Null Safety

### 1. Basic Type Definition with Null Safety

Dart 3.0+ enforces sound null safety at compile time. Every variable is non-nullable by default; use `?` for nullable types.

```dart
import 'package:FraiseQL/FraiseQL.dart';
import 'package:json_annotation/json_annotation.dart';

part 'user.g.dart';  // Generated file for json_serializable

@Type()
@JsonSerializable()
class User {
  final int id;
  final String name;
  final String email;
  final bool isActive;
  final String? bio;  // ✅ Nullable field
  final DateTime createdAt;

  User({
    required this.id,
    required this.name,
    required this.email,
    required this.isActive,
    this.bio,  // ✅ Optional parameter
    required this.createdAt,
  });

  // JSON deserialization
  factory User.fromJson(Map<String, dynamic> json) =>
      _$UserFromJson(json);

  // JSON serialization
  Map<String, dynamic> toJson() => _$UserToJson(this);
}
```text

**Null Safety Patterns**:

- `String` - non-nullable, always has a value
- `String?` - nullable, can be null
- `required this.field` - required named parameter
- `this.field` - optional named parameter with default null
- `late String field` - late initialization (be careful!)

### 2. Complex Types with Nested Objects

```dart
@Type()
@JsonSerializable()
class Address {
  final String street;
  final String city;
  final String state;
  final String postalCode;

  Address({
    required this.street,
    required this.city,
    required this.state,
    required this.postalCode,
  });

  factory Address.fromJson(Map<String, dynamic> json) =>
      _$AddressFromJson(json);
  Map<String, dynamic> toJson() => _$AddressToJson(this);
}

@Type()
@JsonSerializable()
class Company {
  final int id;
  final String name;
  final Address headquarters;  // ✅ Nested non-nullable
  final List<User> employees;  // ✅ List of users
  final String? taxId;  // ✅ Optional field

  Company({
    required this.id,
    required this.name,
    required this.headquarters,
    required this.employees,
    this.taxId,
  });

  factory Company.fromJson(Map<String, dynamic> json) =>
      _$CompanyFromJson(json);
  Map<String, dynamic> toJson() => _$CompanyToJson(this);
}
```text

### 3. Using Late Initialization

For fields initialized after construction, use `late`:

```dart
@Type()
class UserCache {
  final int id;
  final String username;
  late String computedHash;  // Computed after construction
  late DateTime? lastFetch;  // Late nullable field

  UserCache({
    required this.id,
    required this.username,
  }) {
    computedHash = _computeHash(username);
    lastFetch = DateTime.now();
  }

  String _computeHash(String input) => 'hash_$input';
}
```text

## Operations: Queries, Mutations, Subscriptions

### 1. Query Definitions

```dart
@Query()
abstract class UserQueries {
  /// Get a single user by ID
  Future<User?> getUser(int id);

  /// Get all users with optional filtering
  Future<List<User>> listUsers({
    int? limit = 10,
    int? offset = 0,
    String? nameFilter,
  });

  /// Search users by name (case-insensitive)
  Future<List<User>> searchUsers(String query) {
    /// SQL: SELECT * FROM users WHERE name ILIKE '%$query%'
  }

  /// Get user with related orders
  Future<UserWithOrders?> getUserOrders(int userId);
}

// Usage in code:
class UserRepository {
  final FraiseQLClient client;

  UserRepository(this.client);

  Future<User?> fetchUser(int id) async {
    try {
      final user = await client.query.getUser(id);
      return user;  // Type-safe: User | null
    } catch (e) {
      print('Error: $e');
      return null;
    }
  }

  Future<List<User>> searchByName(String name) async {
    return await client.query.searchUsers(name);
  }
}
```text

### 2. Mutation Definitions

```dart
@Mutation()
abstract class UserMutations {
  /// Create a new user
  Future<User> createUser({
    required String name,
    required String email,
    String? bio,
  });

  /// Update user information
  Future<User> updateUser({
    required int id,
    String? name,
    String? email,
    String? bio,
  });

  /// Delete a user
  Future<bool> deleteUser(int id);

  /// Batch create users
  Future<List<User>> bulkCreateUsers(List<CreateUserInput> users);
}

@Type()
@JsonSerializable()
class CreateUserInput {
  final String name;
  final String email;
  final String? bio;

  CreateUserInput({
    required this.name,
    required this.email,
    this.bio,
  });

  factory CreateUserInput.fromJson(Map<String, dynamic> json) =>
      _$CreateUserInputFromJson(json);
  Map<String, dynamic> toJson() => _$CreateUserInputToJson(this);
}

// Usage:
class UserService {
  final FraiseQLClient client;

  UserService(this.client);

  Future<User> createNewUser(String name, String email) async {
    return await client.mutation.createUser(
      name: name,
      email: email,
    );
  }

  Future<bool> removeUser(int id) async {
    return await client.mutation.deleteUser(id);
  }
}
```text

### 3. Subscription Definitions (Real-Time)

```dart
@Subscription()
abstract class UserSubscriptions {
  /// Listen for user updates in real-time
  Stream<UserUpdate> onUserChanged(int userId);

  /// Listen for new users being created
  Stream<User> onUserCreated();

  /// Listen for users being deleted
  Stream<int> onUserDeleted();  // Emits user IDs
}

@Type()
@JsonSerializable()
class UserUpdate {
  final int userId;
  final User user;
  final DateTime timestamp;
  final String changeType;  // 'created', 'updated', 'deleted'

  UserUpdate({
    required this.userId,
    required this.user,
    required this.timestamp,
    required this.changeType,
  });

  factory UserUpdate.fromJson(Map<String, dynamic> json) =>
      _$UserUpdateFromJson(json);
  Map<String, dynamic> toJson() => _$UserUpdateToJson(this);
}

// Usage in Flutter:
class UserListPage extends StatefulWidget {
  @override
  State<UserListPage> createState() => _UserListPageState();
}

class _UserListPageState extends State<UserListPage> {
  late StreamSubscription<UserUpdate> _subscription;
  final List<User> _users = [];

  @override
  void initState() {
    super.initState();
    _subscription = client.subscription.onUserChanged(123).listen((update) {
      setState(() {
        _users
            .removeWhere((u) => u.id == update.userId);
        _users.add(update.user);
      });
    });
  }

  @override
  void dispose() {
    _subscription.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return ListView.builder(
      itemCount: _users.length,
      itemBuilder: (context, index) => UserTile(_users[index]),
    );
  }
}
```text

## Advanced Features

### 1. Fact Tables for Analytics

```dart
@FactTable(
  table: 'user_metrics_fact',
  timeGrain: 'day',
  primaryTimeDimension: 'date',
)
@JsonSerializable()
class UserMetrics {
  final DateTime date;
  final String country;
  final String platform;  // mobile, web, desktop

  @Measure()
  final int activeUsers;

  @Measure()
  final int newSignups;

  @Measure()
  final double avgSessionDuration;

  @Dimension()
  final String cohort;  // user_group or marketing_source

  UserMetrics({
    required this.date,
    required this.country,
    required this.platform,
    required this.activeUsers,
    required this.newSignups,
    required this.avgSessionDuration,
    required this.cohort,
  });

  factory UserMetrics.fromJson(Map<String, dynamic> json) =>
      _$UserMetricsFromJson(json);
  Map<String, dynamic> toJson() => _$UserMetricsToJson(this);
}

// Query fact table with aggregation:
@AggregateQuery()
abstract class AnalyticsQueries {
  /// Get daily active users by country
  Future<List<UserMetrics>> getDailyActiveUsersByCountry({
    required DateTime startDate,
    required DateTime endDate,
  });

  /// Get cohort analysis over time
  Future<List<CohortAnalysis>> getCohortAnalysis({
    required String cohort,
    int? monthsToAnalyze = 12,
  });
}

@Type()
@JsonSerializable()
class CohortAnalysis {
  final DateTime cohortDate;
  final int monthsActive;
  final int userCount;
  final double retentionRate;

  CohortAnalysis({
    required this.cohortDate,
    required this.monthsActive,
    required this.userCount,
    required this.retentionRate,
  });

  factory CohortAnalysis.fromJson(Map<String, dynamic> json) =>
      _$CohortAnalysisFromJson(json);
  Map<String, dynamic> toJson() => _$CohortAnalysisToJson(this);
}
```text

### 2. Role-Based Access Control (RBAC)

```dart
@Type()
@JsonSerializable()
class SecureUser {
  final int id;
  final String name;
  final String email;

  @RequiresScope(['admin'])
  final String? socialSecurityNumber;  // Only admins can access

  @RequiresScope(['admin', 'finance'])
  final double? salary;  // Admin or finance team

  @RequiresScope(['admin', 'user:self'])
  final String? personalNotes;  // Admin or the user themselves

  SecureUser({
    required this.id,
    required this.name,
    required this.email,
    this.socialSecurityNumber,
    this.salary,
    this.personalNotes,
  });

  factory SecureUser.fromJson(Map<String, dynamic> json) =>
      _$SecureUserFromJson(json);
  Map<String, dynamic> toJson() => _$SecureUserToJson(this);
}

// Query with scope-based access:
@Query()
abstract class SecureQueries {
  @RequiresScope(['admin', 'manager'])
  Future<List<SecureUser>> listAllUsers();

  Future<SecureUser?> getCurrentUser();  // No scope needed
}

// Usage:
class AuthService {
  final FraiseQLClient client;
  Set<String> _currentScopes = {};

  AuthService(this.client);

  Future<void> login(String token) async {
    _currentScopes = await client.auth.decodeToken(token);
    client.auth.setToken(token);
  }

  bool hasScope(String scope) => _currentScopes.contains(scope);

  Future<SecureUser?> getCurrentUser() async {
    if (hasScope('admin')) {
      return await client.query.listAllUsers();
    }
    return await client.query.getCurrentUser();
  }
}
```text

### 3. Field Metadata and Validation

```dart
@Type()
@JsonSerializable()
class CreateUserRequest {
  @Field(
    minLength: 2,
    maxLength: 100,
    pattern: r'^[a-zA-Z\s]+$',
    description: 'User full name',
  )
  final String name;

  @Field(
    format: 'email',
    description: 'User email address',
  )
  final String email;

  @Field(
    minLength: 8,
    description: 'Password (min 8 characters)',
    sensitive: true,  // Don't log this
  )
  final String password;

  CreateUserRequest({
    required this.name,
    required this.email,
    required this.password,
  });

  factory CreateUserRequest.fromJson(Map<String, dynamic> json) =>
      _$CreateUserRequestFromJson(json);
  Map<String, dynamic> toJson() => _$CreateUserRequestToJson(this);
}
```text

## Scalar Types

### Type Mappings: Dart ↔ GraphQL

| GraphQL Type | Dart Type | Null Safe | Example |
|--------------|-----------|-----------|---------|
| `String` | `String` | ✅ | `"hello"` |
| `String!` | `String` | ✅ | `"required"` |
| `Int` | `int` | ✅ | `42` |
| `Int!` | `int` | ✅ | `100` |
| `Float` | `double` | ✅ | `3.14` |
| `Float!` | `double` | ✅ | `2.718` |
| `Boolean` | `bool` | ✅ | `true` |
| `Boolean!` | `bool` | ✅ | `false` |
| `ID` | `String` | ✅ | `"user_123"` |
| `DateTime` | `DateTime` | ✅ | `DateTime.now()` |
| `JSON` | `dynamic` or `Map<String, dynamic>` | ⚠️ | `{'key': 'value'}` |
| `[Type]` | `List<Type>` | ✅ | `[1, 2, 3]` |
| `[Type!]!` | `List<Type>` | ✅ | Guaranteed non-empty |
| Custom Type | `CustomType` | ✅ | `User(...)` |

### Custom Scalar Handling

```dart
// Define custom scalars for domain-specific types:
@Type()
@JsonSerializable()
class MoneyAmount {
  final int cents;  // Store as cents to avoid float precision issues

  MoneyAmount({required this.cents});

  double get dollars => cents / 100.0;

  factory MoneyAmount.fromJson(Map<String, dynamic> json) =>
      _$MoneyAmountFromJson(json);
  Map<String, dynamic> toJson() => _$MoneyAmountToJson(this);
}

@Type()
@JsonSerializable()
class GeoPoint {
  final double latitude;
  final double longitude;

  GeoPoint({
    required this.latitude,
    required this.longitude,
  });

  factory GeoPoint.fromJson(Map<String, dynamic> json) =>
      _$GeoPointFromJson(json);
  Map<String, dynamic> toJson() => _$GeoPointToJson(this);
}
```text

## Schema Export Workflow

### 1. Generate Schema.json from Dart Code

FraiseQL provides code generation to extract schema from annotated Dart types:

```bash
# Run build_runner to generate serialization code and schema
dart pub run build_runner build

# This generates:
# - lib/models/user.g.dart (json_serializable)
# - schema.json (FraiseQL schema)
```text

### 2. Compile Schema for Deployment

```bash
# Compile schema on the FraiseQL server
FraiseQL-cli compile schema.json FraiseQL.toml

# Output: schema.compiled.json (ready for runtime)
```text

### 3. Integration with Package Structure

```text
my_fraiseql_app/
├── lib/
│   ├── models/
│   │   ├── user.dart           # @Type() definitions
│   │   ├── user.g.dart         # Generated (git-ignored)
│   │   └── product.dart        # More types
│   ├── queries/
│   │   └── user_queries.dart   # @Query() definitions
│   ├── mutations/
│   │   └── user_mutations.dart # @Mutation() definitions
│   └── main.dart
├── test/
│   └── models/
│       └── user_test.dart      # Type tests
├── pubspec.yaml
├── schema.json                 # Generated (git-ignored)
└── build.yaml                  # build_runner config
```text

## Common Patterns with Flutter Integration

### 1. CRUD Operations in Flutter

```dart
class UserRepository {
  final FraiseQLClient client;

  UserRepository(this.client);

  // Create
  Future<User> create(String name, String email) async {
    return await client.mutation.createUser(
      name: name,
      email: email,
    );
  }

  // Read (single)
  Future<User?> getById(int id) async {
    return await client.query.getUser(id);
  }

  // Read (list)
  Future<List<User>> getAll({int limit = 10, int offset = 0}) async {
    return await client.query.listUsers(
      limit: limit,
      offset: offset,
    );
  }

  // Update
  Future<User> update(int id, {String? name, String? email}) async {
    return await client.mutation.updateUser(
      id: id,
      name: name,
      email: email,
    );
  }

  // Delete
  Future<bool> delete(int id) async {
    return await client.mutation.deleteUser(id);
  }
}

// Widget using Riverpod:
final userProvider = FutureProvider<List<User>>((ref) async {
  final repo = ref.watch(repositoryProvider);
  return repo.getAll();
});

class UserListWidget extends ConsumerWidget {
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final users = ref.watch(userProvider);

    return users.when(
      loading: () => CircularProgressIndicator(),
      error: (error, stack) => Text('Error: $error'),
      data: (userList) => ListView.builder(
        itemCount: userList.length,
        itemBuilder: (context, index) => ListTile(
          title: Text(userList[index].name),
          subtitle: Text(userList[index].email),
        ),
      ),
    );
  }
}
```text

### 2. Pagination Pattern

```dart
class PaginatedUserProvider {
  static final pageSize = 20;

  static final currentPageProvider = StateProvider<int>((ref) => 0);

  static final paginatedUsersProvider =
      FutureProvider.family<List<User>, int>((ref, page) async {
    final repo = ref.watch(repositoryProvider);
    return repo.getAll(
      offset: page * pageSize,
      limit: pageSize,
    );
  });
}

class PaginatedUserList extends ConsumerWidget {
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final currentPage = ref.watch(PaginatedUserProvider.currentPageProvider);
    final users = ref.watch(
      PaginatedUserProvider.paginatedUsersProvider(currentPage),
    );

    return Column(
      children: [
        Expanded(
          child: users.when(
            loading: () => CircularProgressIndicator(),
            error: (error, stack) => Text('Error: $error'),
            data: (userList) => ListView.builder(
              itemCount: userList.length,
              itemBuilder: (context, index) => UserTile(userList[index]),
            ),
          ),
        ),
        Row(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            ElevatedButton(
              onPressed: currentPage > 0
                  ? () => ref.read(
                        PaginatedUserProvider.currentPageProvider.notifier,
                      ).state = currentPage - 1
                  : null,
              child: Text('Previous'),
            ),
            SizedBox(width: 8),
            Text('Page ${currentPage + 1}'),
            SizedBox(width: 8),
            ElevatedButton(
              onPressed: () => ref.read(
                    PaginatedUserProvider.currentPageProvider.notifier,
                  ).state = currentPage + 1,
              child: Text('Next'),
            ),
          ],
        ),
      ],
    );
  }
}
```text

### 3. Search with Debounce

```dart
class SearchProvider {
  static final searchQueryProvider =
      StateProvider<String>((ref) => '');

  static final searchResultsProvider =
      FutureProvider<List<User>>((ref) async {
    final query = ref.watch(searchQueryProvider);

    // Debounce: wait 500ms before searching
    final debounceFuture = Future.delayed(Duration(milliseconds: 500));
    await debounceFuture;

    if (query.isEmpty) return [];

    final repo = ref.watch(repositoryProvider);
    return repo.searchByName(query);
  });
}

class SearchUserWidget extends ConsumerWidget {
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final results = ref.watch(SearchProvider.searchResultsProvider);

    return Column(
      children: [
        TextField(
          onChanged: (query) =>
              ref.read(SearchProvider.searchQueryProvider.notifier).state =
                  query,
          decoration: InputDecoration(
            hintText: 'Search users...',
            prefixIcon: Icon(Icons.search),
          ),
        ),
        Expanded(
          child: results.when(
            loading: () => CircularProgressIndicator(),
            error: (error, stack) => Text('Search error: $error'),
            data: (users) => users.isEmpty
                ? Center(child: Text('No results'))
                : ListView.builder(
                    itemCount: users.length,
                    itemBuilder: (context, index) =>
                        UserTile(users[index]),
                  ),
          ),
        ),
      ],
    );
  }
}
```text

## Error Handling

### Exception Hierarchy and Patterns

```dart
abstract class FraiseQLException implements Exception {
  final String message;
  FraiseQLException(this.message);

  @override
  String toString() => message;
}

class FraiseQLParseException extends FraiseQLException {
  FraiseQLParseException(String message) : super(message);
}

class FraiseQLValidationException extends FraiseQLException {
  FraiseQLValidationException(String message) : super(message);
}

class FraiseQLDatabaseException extends FraiseQLException {
  final String? code;
  FraiseQLDatabaseException(String message, {this.code})
      : super(message);
}

class FraiseQLAuthException extends FraiseQLException {
  FraiseQLAuthException(String message) : super(message);
}

// Usage with try-catch:
Future<User?> safeGetUser(int id) async {
  try {
    return await client.query.getUser(id);
  } on FraiseQLValidationException catch (e) {
    print('Validation error: $e');
    return null;
  } on FraiseQLDatabaseException catch (e) {
    print('Database error: ${e.message} (code: ${e.code})');
    return null;
  } on FraiseQLException catch (e) {
    print('Unknown FraiseQL error: $e');
    return null;
  } catch (e) {
    print('Unexpected error: $e');
    return null;
  }
}

// In Flutter widgets:
class UserForm extends StatefulWidget {
  @override
  State<UserForm> createState() => _UserFormState();
}

class _UserFormState extends State<UserForm> {
  String? _errorMessage;

  Future<void> _submitForm(String name, String email) async {
    try {
      setState(() => _errorMessage = null);

      final user = await client.mutation.createUser(
        name: name,
        email: email,
      );

      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('User created: ${user.name}')),
      );

      Navigator.pop(context, user);
    } on FraiseQLValidationException catch (e) {
      setState(() => _errorMessage = 'Invalid input: $e');
    } on FraiseQLDatabaseException catch (e) {
      setState(() => _errorMessage = 'Database error: ${e.message}');
    } catch (e) {
      setState(() => _errorMessage = 'Unexpected error: $e');
    }
  }

  @override
  Widget build(BuildContext context) {
    return Form(
      child: Column(
        children: [
          if (_errorMessage != null)
            Container(
              color: Colors.red.shade100,
              padding: EdgeInsets.all(12),
              child: Text(
                _errorMessage!,
                style: TextStyle(color: Colors.red.shade900),
              ),
            ),
          // Form fields...
          ElevatedButton(
            onPressed: () => _submitForm('Alice', 'alice@example.com'),
            child: Text('Submit'),
          ),
        ],
      ),
    );
  }
}
```text

## Testing Patterns

### Unit Testing Types and Models

```dart
import 'package:test/test.dart';
import 'package:my_fraiseql_app/models/user.dart';

void main() {
  group('User Model', () {
    test('User creation with all required fields', () {
      final user = User(
        id: 1,
        name: 'Alice',
        email: 'alice@example.com',
        isActive: true,
        createdAt: DateTime(2024, 1, 1),
      );

      expect(user.id, equals(1));
      expect(user.name, equals('Alice'));
      expect(user.email, equals('alice@example.com'));
    });

    test('User JSON serialization round-trip', () {
      final original = User(
        id: 1,
        name: 'Bob',
        email: 'bob@example.com',
        isActive: true,
        bio: 'Bio text',
        createdAt: DateTime(2024, 1, 1),
      );

      final json = original.toJson();
      final restored = User.fromJson(json);

      expect(restored.id, equals(original.id));
      expect(restored.name, equals(original.name));
      expect(restored.bio, equals(original.bio));
    });

    test('User with null optional fields', () {
      final user = User(
        id: 2,
        name: 'Charlie',
        email: 'charlie@example.com',
        isActive: false,
        bio: null,  // ✅ Null safety
        createdAt: DateTime(2024, 2, 1),
      );

      expect(user.bio, isNull);
      expect(user.isActive, isFalse);
    });
  });
}
```text

### Integration Testing with Mock Server

```dart
import 'package:flutter_test/flutter_test.dart';
import 'package:mockito/mockito.dart';

class MockFraiseQLClient extends Mock implements FraiseQLClient {}

void main() {
  group('UserRepository Integration Tests', () {
    late UserRepository repository;
    late MockFraiseQLClient mockClient;

    setUp(() {
      mockClient = MockFraiseQLClient();
      repository = UserRepository(mockClient);
    });

    test('getById returns user from client', () async {
      final mockUser = User(
        id: 1,
        name: 'Test User',
        email: 'test@example.com',
        isActive: true,
        createdAt: DateTime.now(),
      );

      when(mockClient.query.getUser(1))
          .thenAnswer((_) async => mockUser);

      final result = await repository.getById(1);

      expect(result, equals(mockUser));
      verify(mockClient.query.getUser(1)).called(1);
    });

    test('create handles database exceptions', () async {
      when(mockClient.mutation.createUser(
        name: 'Alice',
        email: 'alice@example.com',
      )).thenThrow(
        FraiseQLDatabaseException('Duplicate email'),
      );

      expect(
        () => repository.create('Alice', 'alice@example.com'),
        throwsA(isA<FraiseQLDatabaseException>()),
      );
    });
  });
}
```text

## See Also

- [FraiseQL Python SDK Reference](python-reference.md) - Schema authoring guide
- [FraiseQL Swift SDK Reference](swift-reference.md) - iOS/macOS integration
- [Flutter State Management Guide](../README.md) - Provider, Riverpod, GetX
- [JSON Serialization Best Practices](../../integrations/README.md) - json_serializable patterns
- [Null Safety in Dart](https://dart.dev/null-safety) - Official Dart null safety documentation
- [Flutter Official Documentation](https://flutter.dev) - Flutter framework reference
- [FraiseQL Architecture](../../architecture/README.md) - System design principles

---

---

## Troubleshooting

### Common Setup Issues

#### Pub Package Issues

**Issue**: `Could not find package FraiseQL`

**Solution**:

```yaml
# pubspec.yaml
dependencies:
  FraiseQL: ^2.0.0
```text

```bash
pub get
pub upgrade
```text

#### Null Safety Issues

**Issue**: `The type 'User?' must be assignable to 'User'`

**Enable null safety**:

```yaml
# pubspec.yaml
environment:
  SDK: '>=3.0.0 <4.0.0'
```text

**Use correct nullability**:

```dart
// ✅ Nullable
User? user;
String? middleName;

// ✅ Non-null
User user;
String email;
```text

#### Async/Await Issues

**Issue**: `The expression here has a type of 'Future<..>'`

**Solution - Use await**:

```dart
// ❌ Wrong - not awaiting
var result = server.execute(query);

// ✅ Correct
var result = await server.execute(query);
```text

#### Build Runner Issues

**Issue**: `Unable to run build`

**Solution**:

```bash
pub run build_runner build
pub run build_runner watch
```text

---

### Type System Issues

#### Type Conversion Issues

**Issue**: `The argument type 'Map<String, dynamic>' can't be assigned to parameter type 'Map<String, Object>'`

**Solution - Cast properly**:

```dart
// ✅ Correct cast
final variables = <String, Object>{
  'id': 123,
  'name': 'Alice'
};

final result = await server.execute(
  query: query,
  variables: variables
);
```text

#### Null Safety Issues

**Issue**: `null can't be assigned to non-null type`

**Solution - Check null before use**:

```dart
// ✅ Check first
if (user != null) {
  print(user.email);  // Safe
}

// ✅ Or use optional chaining
print(user?.email ?? 'Unknown');
```text

#### Generic Type Issues

**Issue**: `The type 'T' is not known to be a subtype`

**Solution - Use concrete types**:

```dart
// ❌ Won't work
class Box<T> {
  T value;
}

// ✅ Use concrete types
class UserBox {
  User value;
}
```text

---

### Runtime Errors

#### Network Issues

**Issue**: `SocketException: Failed to connect`

**Check connectivity**:

```dart
// Add connectivity_plus
const http = 'http://localhost:8080/graphql';
final result = await http.post(Uri.parse(http));
```text

#### JSON Deserialization Issues

**Issue**: `type 'Null' is not a subtype of type 'String'`

**Solution - Handle null safely**:

```dart
// ✅ Use generated json_serializable
@JsonSerializable()
class User {
  final int id;
  final String name;
  final String? middleName;

  User({
    required this.id,
    required this.name,
    this.middleName,
  });

  factory User.fromJson(Map<String, dynamic> json) =>
      _$UserFromJson(json);
}
```text

#### Future Issues

**Issue**: `NoSuchMethodError: method 'then' called on null`

**Solution - Always return Future**:

```dart
// ❌ Wrong
Future<User>? getUser() {
  return server.execute(query);  // Nullable Future
}

// ✅ Correct
Future<User> getUser() {
  return server.execute(query);
}
```text

---

### Performance Issues

#### Build Time

**Issue**: Build takes >2 minutes

**Clean and rebuild**:

```bash
flutter clean
flutter pub get
flutter build
```text

#### Memory Usage

**Issue**: App uses >200MB

**Profile with DevTools**:

```bash
flutter run --profile
```text

**Optimize**:

- Paginate large lists
- Use const constructors
- Dispose controllers

#### Network Timeouts

**Issue**: `SocketException: Connection reset by peer`

**Increase timeout**:

```dart
final client = http.Client();
final response = await client.post(
  Uri.parse('http://localhost:8080/graphql'),
  body: queryJson,
).timeout(Duration(seconds: 60));
```text

---

### Debugging Techniques

#### Print Debugging

```dart
debugPrint('Query: $query');
debugPrint('Result: $result');
```text

#### DevTools

```bash
flutter pub global activate devtools
devtools
# Opens browser at http://localhost:9100
```text

#### Logging

```dart
import 'package:logger/logger.dart';

final logger = Logger();

logger.d('Debug message');
logger.i('Info message');
logger.e('Error', error: exception);
```text

---

### Getting Help

Provide: 1. Dart version: `dart --version`
2. Flutter version: `flutter --version`
3. FraiseQL version: `pub list FraiseQL`
4. Error message
5. Minimal code example

---

**Last Updated**: 2026-02-05 | **Dart SDK Version**: 2.0.0+ | **Flutter**: 3.0+

For issues, questions, or contributions, visit the [FraiseQL GitHub repository](https://github.com/FraiseQL/FraiseQL).
