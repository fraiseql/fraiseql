# FraiseQL Swift SDK Reference

**Status**: Production-Ready | **Swift Version**: 5.9+ | **Xcode**: 15.0+ | **iOS/macOS**: 13.0+

Complete API reference for the FraiseQL Swift SDK. Build type-safe GraphQL APIs using Swift 5.9's strongly-typed ecosystem. Native SwiftUI integration, Codable serialization, async/await concurrency, and full Apple ecosystem support—from iOS apps to macOS servers.

## Installation

### Swift Package Manager (SPM)

Add FraiseQL to your `Package.swift`:

```swift
// Package.swift
let package = Package(
  name: "MyApp",
  platforms: [
    .iOS(.v13),
    .macOS(.v10_15),
    .tvOS(.v13),
    .watchOS(.v6)
  ],
  dependencies: [
    .package(url: "https://github.com/fraiseql/fraiseql-swift.git", from: "2.0.0")
  ],
  targets: [
    .target(
      name: "MyApp",
      dependencies: [
        .product(name: "FraiseQL", package: "fraiseql-swift")
      ]
    )
  ]
)
```

Or in Xcode: File → Add Packages → Enter repository URL.

**Requirements:**
- Swift 5.9 or later (async/await, typed throws)
- Xcode 15.0 or later
- iOS 13.0+, macOS 10.15+, tvOS 13.0+, watchOS 6.0+
- Foundation framework (standard library)

**Supported Platforms:**
- iOS (primary: iPhone, iPad)
- macOS (server or desktop)
- tvOS (Apple TV)
- watchOS (Apple Watch)
- Linux (Swift on Server, no GUI frameworks)

## Quick Reference Table

| Feature | Attribute | Purpose |
|---------|-----------|---------|
| **Types** | `@Type` | Define GraphQL object types |
| **Codable Structs** | `Codable` protocol | JSON serialization/deserialization |
| **Queries** | `@Query` | Read operations (SELECT) |
| **Mutations** | `@Mutation` | Write operations (INSERT/UPDATE/DELETE) |
| **Subscriptions** | `@Subscription` | Real-time event streams |
| **Fact Tables** | `@FactTable` | Analytics tables with measures/dimensions |
| **Aggregate Queries** | `@AggregateQuery` | GROUP BY aggregations |
| **Field Security** | `requiresScope`, `deprecated` | Field-level access control |
| **Schema Export** | `exportSchema()` | Generate schema.json |
| **Async/Await** | `async/await` | Concurrency without callbacks |
| **Actors** | `@FraiseQLActor` | Thread-safe database operations |

## Type System

### Basic Type Definition with Codable

```swift
import FraiseQL

@Type
struct User: Codable {
    let id: Int
    let name: String
    let email: String
    let isActive: Bool

    enum CodingKeys: String, CodingKey {
        case id, name, email
        case isActive = "is_active"
    }
}

// Automatic Codable serialization:
let userData = """
{
  "id": 1,
  "name": "Alice",
  "email": "alice@example.com",
  "is_active": true
}
""".data(using: .utf8)!

let user = try JSONDecoder().decode(User.self, from: userData)
print(user.name) // "Alice"
```

### Optional and Nullable Types

```swift
// Nullable (can be nil in GraphQL response)
@Type
struct UserProfile: Codable {
    let id: Int
    let name: String
    let middleName: String?  // Optional field (nullable in GraphQL)
    let bio: String?
    let avatarUrl: URL?

    enum CodingKeys: String, CodingKey {
        case id, name
        case middleName = "middle_name"
        case bio
        case avatarUrl = "avatar_url"
    }
}

// Swift's Optional<T> (equivalent to T?)
let profile: UserProfile? = nil  // Optional parameter

// Non-optional (required in GraphQL)
let user: User = User(id: 1, name: "Bob", email: "bob@example.com", isActive: true)
```

### Nested Types and Relationships

```swift
@Type
struct Post: Codable {
    let id: Int
    let title: String
    let author: User  // Nested type reference
    let comments: [Comment]  // Array of nested types
    let metadata: PostMetadata?
}

@Type
struct Comment: Codable {
    let id: Int
    let text: String
    let authorId: Int
}

@Type
struct PostMetadata: Codable {
    let viewCount: Int
    let lastEditedAt: Date
}
```

### Enum Types

```swift
enum OrderStatus: String, Codable {
    case pending = "pending"
    case processing = "processing"
    case shipped = "shipped"
    case delivered = "delivered"
    case cancelled = "cancelled"
}

@Type
struct Order: Codable {
    let id: Int
    let status: OrderStatus
    let totalPrice: Decimal
    let items: [OrderItem]
}

// Safe enum handling in Swift
let order: Order = Order(
    id: 1,
    status: .shipped,
    totalPrice: Decimal(string: "99.99")!,
    items: []
)

// JSON encoding preserves string values
let encoder = JSONEncoder()
let json = try encoder.encode(order)
```

### Generic Collections and Dictionaries

```swift
@Type
struct QueryResult: Codable {
    let data: [String: AnyCodable]  // JSON-compatible dictionary
    let metadata: [String: String]
}

// Decodable helper for untyped JSON
struct AnyCodable: Codable {
    let value: Any

    func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        try container.encode(value as? String ?? String(describing: value))
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()
        if let string = try? container.decode(String.self) {
            self.value = string
        } else if let int = try? container.decode(Int.self) {
            self.value = int
        } else {
            self.value = NSNull()
        }
    }
}
```

## Operations

### Query Operations with Async/Await

```swift
import FraiseQL

@Query(sqlSource: "v_users")
func users(limit: Int = 10, offset: Int = 0) async throws -> [User] {
    // Compiled to SQL query at build time
    throw FraiseQLError.notImplemented("Query compilation happens at build time")
}

// Modern async/await usage in SwiftUI
struct UserListView: View {
    @State var users: [User] = []
    @State var isLoading = false
    @State var error: Error?

    var body: some View {
        List {
            ForEach(users, id: \.id) { user in
                VStack(alignment: .leading) {
                    Text(user.name).font(.headline)
                    Text(user.email).font(.caption)
                }
            }
        }
        .task {
            await loadUsers()
        }
    }

    private func loadUsers() async {
        isLoading = true
        do {
            users = try await users(limit: 20, offset: 0)
        } catch {
            self.error = error
        }
        isLoading = false
    }
}
```

### Single Item Query

```swift
@Query(sqlSource: "fn_get_user")
func user(id: Int) async throws -> User? {
    throw FraiseQLError.notImplemented("Compiled at build time")
}

// Usage
let user = try await user(id: 42)
if let user = user {
    print("Found user: \(user.name)")
} else {
    print("User not found")
}
```

### Mutation Operations

```swift
@Mutation(sqlSource: "fn_create_user", operation: "CREATE")
func createUser(email: String, name: String) async throws -> User {
    throw FraiseQLError.notImplemented("Compiled at build time")
}

@Mutation(sqlSource: "fn_update_user", operation: "UPDATE")
func updateUser(id: Int, email: String?, name: String?) async throws -> User? {
    throw FraiseQLError.notImplemented("Compiled at build time")
}

@Mutation(sqlSource: "fn_delete_user", operation: "DELETE")
func deleteUser(id: Int) async throws -> Bool {
    throw FraiseQLError.notImplemented("Compiled at build time")
}

// CRUD in SwiftUI with @State and @StateObject
@MainActor
class UserViewModel: ObservableObject {
    @Published var users: [User] = []
    @Published var isLoading = false
    @Published var error: Error?

    func createNewUser(email: String, name: String) async {
        isLoading = true
        do {
            let newUser = try await createUser(email: email, name: name)
            users.append(newUser)
        } catch {
            self.error = error
        }
        isLoading = false
    }

    func deleteUserWithId(_ id: Int) async {
        do {
            let success = try await deleteUser(id: id)
            if success {
                users.removeAll { $0.id == id }
            }
        } catch {
            self.error = error
        }
    }
}
```

### Subscription Operations (Real-Time)

```swift
@Subscription(topic: "userCreated", operation: "CREATE")
func onUserCreated() async throws -> AsyncStream<User> {
    throw FraiseQLError.notImplemented("Compiled at build time")
}

// SwiftUI integration with AsyncStream
struct RealtimeUserView: View {
    @State var newUsers: [User] = []

    var body: some View {
        VStack {
            Text("New Users: \(newUsers.count)")
            List(newUsers, id: \.id) { user in
                Text(user.name)
            }
        }
        .task {
            await subscribeToNewUsers()
        }
    }

    private func subscribeToNewUsers() async {
        do {
            for try await user in try await onUserCreated() {
                newUsers.append(user)
            }
        } catch {
            print("Subscription error: \(error)")
        }
    }
}
```

## Advanced Features

### Fact Tables for Analytics

```swift
@FactTable(
    tableName: "tf_sales",
    measures: ["revenue", "quantity", "cost"],
    dimensionPaths: [
        .init(name: "region", jsonPath: "data->>'region'", dataType: "text"),
        .init(name: "category", jsonPath: "data->>'category'", dataType: "text"),
        .init(name: "saleDate", jsonPath: "data->>'date'", dataType: "date")
    ]
)
@Type
struct Sale: Codable {
    let id: Int
    let revenue: Decimal
    let quantity: Int
    let cost: Decimal
    let customerId: Int
}

@AggregateQuery(factTable: "tf_sales", autoGroupBy: true, autoAggregates: true)
func salesSummary(
    groupBy: [String]? = nil,
    where: String? = nil,
    limit: Int = 100
) async throws -> [SalesAggregation] {
    throw FraiseQLError.notImplemented("Compiled at build time")
}

@Type
struct SalesAggregation: Codable {
    let region: String?
    let category: String?
    let totalRevenue: Decimal
    let totalQuantity: Int
    let averageCost: Decimal
}

// SwiftUI chart integration
import Charts

struct SalesChartView: View {
    @State var aggregations: [SalesAggregation] = []

    var body: some View {
        Chart(aggregations, id: \.region) { agg in
            BarMark(
                x: .value("Region", agg.region ?? "Unknown"),
                y: .value("Revenue", agg.totalRevenue.doubleValue)
            )
        }
        .task {
            do {
                aggregations = try await salesSummary(groupBy: ["region"])
            } catch {
                print("Error: \(error)")
            }
        }
    }
}
```

### Role-Based Access Control (RBAC)

```swift
@Type
struct SensitiveUser: Codable {
    let id: Int
    let name: String
    let email: String

    let salary: Decimal?  // requiresScope: ["read:User.salary", "admin"]
    let ssn: String?      // requiresScope: ["pii:read"]

    enum CodingKeys: String, CodingKey {
        case id, name, email, salary, ssn
    }
}

// Swift SDK validates scopes before serialization
class SecureUserService {
    private let authToken: String

    func loadSensitiveUser(id: Int) async throws -> SensitiveUser? {
        // Auth token is validated server-side
        // Fields requiring scopes are omitted if user lacks permission
        let response = try await fetchUser(id: id, token: authToken)
        return try JSONDecoder().decode(SensitiveUser.self, from: response)
    }
}
```

### Field-Level Metadata

```swift
@Type
struct Product: Codable {
    let id: Int
    let name: String

    // Deprecated field with migration path
    let oldPrice: Decimal?  // deprecated: "Use pricing.current instead"
    let pricing: PricingInfo

    enum CodingKeys: String, CodingKey {
        case id, name
        case oldPrice = "old_price"
        case pricing
    }
}

@Type
struct PricingInfo: Codable {
    let current: Decimal
    let original: Decimal?
    let discountPercent: Int?
}

// Client-side deprecation warning
#if DEBUG
func loadProduct(id: Int) async throws -> Product {
    let product = try await getProduct(id: id)
    if product.oldPrice != nil {
        print("⚠️ WARNING: Product.oldPrice is deprecated. Use pricing.current instead.")
    }
    return product
}
#endif
```

### SwiftUI Integration with @StateObject

```swift
@MainActor
final class UserRepository: NSObject, ObservableObject {
    @Published var users: [User] = []
    @Published var selectedUser: User?
    @Published var isLoading = false
    @Published var error: Error?

    override init() {
        super.init()
        Task {
            await loadUsers()
        }
    }

    func loadUsers() async {
        isLoading = true
        defer { isLoading = false }

        do {
            users = try await users(limit: 100)
        } catch {
            self.error = error
        }
    }

    func selectUser(_ user: User) {
        selectedUser = user
    }
}

struct ContentView: View {
    @StateObject var userRepo = UserRepository()

    var body: some View {
        NavigationView {
            List(userRepo.users, id: \.id, selection: $userRepo.selectedUser) { user in
                NavigationLink(destination: UserDetailView(user: user)) {
                    UserRowView(user: user)
                }
            }
            .navigationTitle("Users")
            .overlay {
                if userRepo.isLoading {
                    ProgressView()
                }
            }
            .alert("Error", isPresented: .constant(userRepo.error != nil)) {
                Button("OK") { userRepo.error = nil }
            } message: {
                Text(userRepo.error?.localizedDescription ?? "Unknown error")
            }
        }
    }
}
```

### Thread-Safe Database Operations with Actors

```swift
@FraiseQLActor
final class DatabaseService {
    private var cache: [Int: User] = [:]

    func fetchUser(id: Int) async throws -> User {
        // Actor isolation ensures thread safety
        if let cached = cache[id] {
            return cached
        }

        let user = try await user(id: id)
        if let user = user {
            cache[id] = user
        }
        return user ?? User(id: id, name: "", email: "", isActive: false)
    }

    func clearCache() {
        cache.removeAll()
    }

    nonisolated func getCacheSize() -> Int {
        // Can be called from any thread (read-only)
        return cache.count
    }
}

// Usage with MainActor isolation
@MainActor
class AppDelegate: UIResponder, UIApplicationDelegate {
    private let dbService = DatabaseService()

    func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil
    ) -> Bool {
        Task {
            let user = try await dbService.fetchUser(id: 1)
            print("User: \(user.name)")
        }
        return true
    }
}
```

## Scalar Types

FraiseQL Swift SDK maps GraphQL scalars to native Swift types:

| GraphQL Type | Swift Type | Foundation | Example |
|--------------|-----------|-----------|---------|
| `Int` | `Int` | - | `42` |
| `Float` | `Double` | - | `3.14` |
| `String` | `String` | - | `"hello"` |
| `Boolean` | `Bool` | - | `true` |
| `ID` | `String` | - | `"user-123"` |
| `DateTime` | `Date` | `Foundation` | `Date()` |
| `Date` | `Date` | `Foundation` | `Date()` |
| `Time` | `Date` | `Foundation` | `Date()` |
| `Decimal` | `Decimal` | `Foundation` | `Decimal(string: "99.99")` |
| `JSON` | `Codable` | - | `[String: AnyCodable]` |
| `Email` | `String` | - | `"user@example.com"` |
| `URL` | `URL` | `Foundation` | `URL(string: "https://example.com")` |
| `UUID` | `UUID` | `Foundation` | `UUID()` |
| `Phone` | `String` | - | `"+1-555-0100"` |
| `IPv4` | `String` | - | `"192.168.1.1"` |
| `IPv6` | `String` | - | `"2001:0db8:85a3::8a2e:0370:7334"` |

## Schema Export

### Export to File

```swift
import FraiseQL

// In your schema definition module
@main
struct SchemaExporter {
    static func main() throws {
        try FraiseQL.exportSchema(
            to: URL(fileURLWithPath: "schema.json"),
            pretty: true
        )
        print("Schema exported to schema.json")
    }
}

// Run from command line
swift run fraiseql-schema-export
```

### Embed in SPM Target

```swift
// Sources/FraiseQLSchema/main.swift
import FraiseQL

struct SchemaBuilder {
    static func build() throws -> [String: Any] {
        var schema = [String: Any]()

        // Register all types, queries, mutations
        // ...

        return schema
    }
}

// Product definition: library + executable
let package = Package(
    products: [
        .library(name: "FraiseQLSchema", targets: ["FraiseQLSchema"]),
        .executable(name: "schema-builder", targets: ["SchemaBuilder"]),
    ]
)
```

### Get Schema as Dictionary

```swift
let schema = try FraiseQL.getSchemaDict()
print("Queries: \(schema["queries"] as? [[String: Any]] ?? [])")
print("Mutations: \(schema["mutations"] as? [[String: Any]] ?? [])")

// Export as JSON string
let encoder = JSONEncoder()
encoder.outputFormatting = .prettyPrinted
let jsonData = try encoder.encode(schema)
let jsonString = String(data: jsonData, encoding: .utf8)!
print(jsonString)
```

### Schema Structure

```json
{
  "types": [
    {
      "name": "User",
      "kind": "OBJECT",
      "fields": [
        { "name": "id", "type": "ID!", "nullable": false },
        { "name": "name", "type": "String!", "nullable": false }
      ]
    }
  ],
  "queries": [
    {
      "name": "users",
      "returnType": "User",
      "returnsList": true,
      "nullable": false,
      "args": []
    }
  ],
  "mutations": [],
  "subscriptions": []
}
```

## Type Mapping

Swift to GraphQL type conversion with Codable:

| Swift Type | GraphQL | Nullable | Codable |
|-----------|---------|----------|---------|
| `Int` | `Int!` | Required | ✅ |
| `Double` | `Float!` | Required | ✅ |
| `String` | `String!` | Required | ✅ |
| `Bool` | `Boolean!` | Required | ✅ |
| `UUID` | `ID!` | Required | ✅ |
| `Date` | `DateTime!` | Required | ✅ |
| `Decimal` | `Decimal!` | Required | ✅ |
| `Int?` | `Int` | Nullable | ✅ |
| `[Int]` | `[Int!]!` | List (required items) | ✅ |
| `[Int?]?` | `[Int]` | List (nullable items & list) | ✅ |
| `[User]` | `[User!]!` | List of custom types | ✅ |
| `[String: String]` | `JSON` | Untyped JSON | ✅ |
| `User` (struct) | `User` (custom type) | Required | ✅ |

## Common Patterns

### CRUD with SwiftUI

```swift
// Create
struct CreateUserView: View {
    @State var email = ""
    @State var name = ""
    @State var isLoading = false

    var body: some View {
        Form {
            TextField("Name", text: $name)
            TextField("Email", text: $email)
            Button("Create") {
                Task {
                    isLoading = true
                    do {
                        let _ = try await createUser(email: email, name: name)
                        // Success: navigate back
                    } catch {
                        print("Error: \(error)")
                    }
                    isLoading = false
                }
            }
            .disabled(isLoading || email.isEmpty || name.isEmpty)
        }
    }
}

// Read (fetch list)
struct UserListView: View {
    @State var users: [User] = []

    var body: some View {
        List(users, id: \.id) { user in
            VStack(alignment: .leading) {
                Text(user.name)
                Text(user.email).font(.caption)
            }
        }
        .onAppear {
            Task {
                users = try await users(limit: 50)
            }
        }
    }
}

// Update
struct EditUserView: View {
    let user: User
    @State var name: String = ""
    @State var email: String = ""

    var body: some View {
        Form {
            TextField("Name", text: $name)
            TextField("Email", text: $email)
            Button("Save") {
                Task {
                    let _ = try await updateUser(
                        id: user.id,
                        email: email.isEmpty ? nil : email,
                        name: name.isEmpty ? nil : name
                    )
                }
            }
        }
        .onAppear {
            name = user.name
            email = user.email
        }
    }
}

// Delete with confirmation
.swipeActions(edge: .trailing, allowsFullSwipe: true) {
    Button(role: .destructive) {
        Task {
            let success = try await deleteUser(id: user.id)
            if success {
                users.removeAll { $0.id == user.id }
            }
        }
    } label: {
        Label("Delete", systemImage: "trash")
    }
}
```

### Pagination with SwiftUI

```swift
struct PaginatedUserView: View {
    @State var users: [User] = []
    @State var currentPage = 0
    @State var isLoading = false
    let pageSize = 20

    var body: some View {
        NavigationView {
            List(users, id: \.id) { user in
                Text(user.name)
                    .onAppear {
                        if user.id == users.last?.id {
                            Task {
                                await loadNextPage()
                            }
                        }
                    }
            }
            .navigationTitle("Users")
            .toolbar {
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button(action: { Task { await loadNextPage() } }) {
                        Image(systemName: "arrow.down")
                    }
                }
            }
        }
        .task {
            await loadFirstPage()
        }
    }

    private func loadFirstPage() async {
        isLoading = true
        do {
            users = try await users(limit: pageSize, offset: 0)
            currentPage = 0
        } catch {
            print("Error: \(error)")
        }
        isLoading = false
    }

    private func loadNextPage() async {
        isLoading = true
        do {
            let nextUsers = try await users(
                limit: pageSize,
                offset: (currentPage + 1) * pageSize
            )
            users.append(contentsOf: nextUsers)
            currentPage += 1
        } catch {
            print("Error: \(error)")
        }
        isLoading = false
    }
}
```

### Filtering and Search

```swift
struct FilteredUserView: View {
    @State var users: [User] = []
    @State var searchText = ""

    var filteredUsers: [User] {
        if searchText.isEmpty {
            return users
        }
        return users.filter { user in
            user.name.localizedCaseInsensitiveContains(searchText) ||
            user.email.localizedCaseInsensitiveContains(searchText)
        }
    }

    var body: some View {
        NavigationView {
            List(filteredUsers, id: \.id) { user in
                Text(user.name)
            }
            .searchable(text: $searchText, prompt: "Search users")
            .navigationTitle("Users")
        }
        .task {
            do {
                users = try await users(limit: 100)
            } catch {
                print("Error: \(error)")
            }
        }
    }
}
```

## Error Handling

### FraiseQL Error Types

```swift
enum FraiseQLError: Error, LocalizedError {
    case parseError(message: String, location: String?)
    case validationError(message: String, path: String?)
    case authenticationError(message: String)
    case authorizationError(message: String)
    case notFoundError(message: String, resource: String?)
    case databaseError(message: String, code: String?)
    case rateLimitError(message: String, retryAfter: Int?)
    case decodingError(DecodingError)
    case networkError(URLError)

    var errorDescription: String? {
        switch self {
        case .parseError(let message, _):
            return "Parse error: \(message)"
        case .validationError(let message, let path):
            return "Validation error\(path.map { " at \($0)" } ?? ""): \(message)"
        case .authenticationError(let message):
            return "Not authenticated: \(message)"
        case .authorizationError(let message):
            return "Not authorized: \(message)"
        case .databaseError(let message, _):
            return "Database error: \(message)"
        case .rateLimitError(let message, _):
            return "Rate limited: \(message)"
        default:
            return nil
        }
    }
}

// Usage with proper error handling
func loadUser(id: Int) async throws -> User? {
    do {
        return try await user(id: id)
    } catch let error as FraiseQLError {
        switch error {
        case .authenticationError:
            print("User needs to authenticate")
        case .authorizationError:
            print("User lacks permission")
        case .notFoundError:
            print("User not found")
        case .rateLimitError(_, let retryAfter):
            print("Rate limited. Retry after \(retryAfter ?? 60) seconds")
        default:
            print("Error: \(error.errorDescription ?? "Unknown")")
        }
        throw error
    }
}
```

### SwiftUI Error Display

```swift
struct UserDetailView: View {
    @State var user: User?
    @State var error: Error?
    @State var isLoading = false
    let userId: Int

    var body: some View {
        ZStack {
            if isLoading {
                ProgressView()
            } else if let user = user {
                VStack(alignment: .leading) {
                    Text(user.name).font(.title)
                    Text(user.email).font(.caption)
                }
            } else if let error = error {
                VStack {
                    Image(systemName: "exclamationmark.triangle")
                        .font(.largeTitle)
                        .foregroundColor(.red)
                    Text("Error loading user")
                    Text(error.localizedDescription)
                        .font(.caption)
                        .multilineTextAlignment(.center)
                    Button("Retry") {
                        Task { await loadUser() }
                    }
                }
                .padding()
            }
        }
        .task {
            await loadUser()
        }
    }

    private func loadUser() async {
        isLoading = true
        error = nil
        defer { isLoading = false }

        do {
            user = try await user(id: userId)
        } catch {
            self.error = error
        }
    }
}
```

## Testing

### XCTest Patterns

```swift
import XCTest
import FraiseQL

final class FraiseQLSchemaTests: XCTestCase {

    func testUserTypeDefinition() async throws {
        let user = User(
            id: 1,
            name: "Alice",
            email: "alice@example.com",
            isActive: true
        )

        XCTAssertEqual(user.id, 1)
        XCTAssertEqual(user.name, "Alice")
        XCTAssertTrue(user.isActive)
    }

    func testUserCodable() throws {
        let json = """
        {
          "id": 2,
          "name": "Bob",
          "email": "bob@example.com",
          "is_active": false
        }
        """.data(using: .utf8)!

        let user = try JSONDecoder().decode(User.self, from: json)
        XCTAssertEqual(user.id, 2)
        XCTAssertEqual(user.name, "Bob")
        XCTAssertFalse(user.isActive)
    }

    func testUserEncoding() throws {
        let user = User(
            id: 3,
            name: "Charlie",
            email: "charlie@example.com",
            isActive: true
        )

        let data = try JSONEncoder().encode(user)
        let decoded = try JSONDecoder().decode(User.self, from: data)

        XCTAssertEqual(user.id, decoded.id)
        XCTAssertEqual(user.name, decoded.name)
    }

    func testQueryAsync() async throws {
        let users = try await users(limit: 10)
        XCTAssertGreaterThan(users.count, 0)
    }

    @MainActor
    func testViewModelIntegration() async {
        let viewModel = UserRepository()

        XCTAssertEqual(viewModel.users.count, 0)

        await viewModel.loadUsers()

        if viewModel.error == nil {
            XCTAssertGreaterThan(viewModel.users.count, 0)
        }
    }
}
```

### Snapshot Testing for SwiftUI

```swift
import SnapshotTesting
import SwiftUI

final class UserViewSnapshotTests: XCTestCase {

    func testUserRowSnapshot() {
        let user = User(
            id: 1,
            name: "Test User",
            email: "test@example.com",
            isActive: true
        )

        let view = UserRowView(user: user)
        assertSnapshot(matching: view, as: .image)
    }
}
```

## iOS/macOS Platform Differences

### Platform-Specific Code

```swift
#if os(iOS)
import UIKit

@MainActor
class IOSUserViewController: UIViewController {
    func loadUsers() async {
        do {
            let users = try await users(limit: 20)
            print("Loaded \(users.count) users on iOS")
        } catch {
            self.showError(error)
        }
    }

    private func showError(_ error: Error) {
        let alert = UIAlertController(
            title: "Error",
            message: error.localizedDescription,
            preferredStyle: .alert
        )
        alert.addAction(UIAlertAction(title: "OK", style: .default))
        present(alert, animated: true)
    }
}

#elseif os(macOS)
import Cocoa

class MacOSUserWindowController: NSWindowController {
    func loadUsers() async {
        do {
            let users = try await users(limit: 100)
            print("Loaded \(users.count) users on macOS")
        } catch {
            self.showError(error)
        }
    }

    private func showError(_ error: Error) {
        let alert = NSAlert()
        alert.messageText = "Error"
        alert.informativeText = error.localizedDescription
        alert.runModal()
    }
}

#endif
```

### SwiftUI Platform Adaptation

```swift
struct AdaptiveUserListView: View {
    @State var users: [User] = []

    var body: some View {
        #if os(iOS)
        NavigationView {
            userList
                .navigationTitle("Users")
        }
        #else
        NavigationSplitView {
            userList
        } detail: {
            Text("Select a user")
        }
        #endif
    }

    var userList: some View {
        List(users, id: \.id) { user in
            VStack(alignment: .leading) {
                Text(user.name).font(.headline)
                Text(user.email).font(.caption)
            }
        }
    }
}
```

## See Also

- [Python SDK Reference](./python-reference.md)
- [TypeScript SDK Reference](./typescript-reference.md)
- [GraphQL Scalars Reference](../../reference/scalars.md)
- [Security & RBAC Guide](../../guides/security-and-rbac.md)
- [Apple Developer: Async/Await](https://developer.apple.com/documentation/swift/concurrency)
- [Apple Developer: Codable](https://developer.apple.com/documentation/foundation/codable)
- [SwiftUI Documentation](https://developer.apple.com/xcode/swiftui/)
- [Swift Package Manager](https://swift.org/package-manager/)
- [FraiseQL Swift SDK on GitHub](https://github.com/fraiseql/fraiseql-swift)

---

## Troubleshooting

### Common Setup Issues

#### SPM Package Issues

**Issue**: `error: no such module or it has no products`

**Solution**:
```swift
// Package.swift
.package(url: "https://github.com/fraiseql/fraiseql-swift.git", .upToNextMajor(from: "2.0.0"))
```

```bash
swift package update
swift package resolve
```

#### Swift Version Issues

**Issue**: `Unsupported Swift language version`

**Check version** (5.7+ required):
```bash
swift --version
```

**Update Xcode**:
```bash
xcode-select --install
softwareupdate -i -a
```

#### Build Issues

**Issue**: `error: build system failure`

**Solution - Clean and rebuild**:
```bash
swift package clean
swift package update
swift build
```

#### iOS/macOS Compatibility

**Issue**: `Platform not supported`

**Check minimum deployment**:
```swift
let minimumOS = "13.0"  // iOS
let minimumMacOS = "10.15"
```

---

### Type System Issues

#### Codable Issues

**Issue**: `error: type 'User' does not conform to protocol 'Decodable'`

**Solution - Implement Codable**:
```swift
// ✅ Correct
struct User: Codable {
    let id: Int
    let email: String
    let middleName: String?

    enum CodingKeys: String, CodingKey {
        case id, email
        case middleName = "middle_name"
    }
}

// ✅ Or use @Codable macro (Swift 5.10+)
@Codable
struct User {
    let id: Int
    let email: String
}
```

#### Optional Issues

**Issue**: `Cannot convert value of type 'String' to expected argument type 'String?'`

**Solution - Handle optionals properly**:
```swift
// ✅ Explicit optionals
struct User {
    let email: String      // Non-optional
    let middleName: String?  // Optional
}

// ✅ Safe unwrapping
if let name = user.middleName {
    print(name)
}
```

#### Generic Type Issues

**Issue**: `error: cannot specialize non-generic type`

**Solution - Use concrete types**:
```swift
// ❌ Won't work - generics
struct Box<T: Codable> {
    let value: T
}

// ✅ Use concrete types
struct UserBox {
    let value: User
}
```

#### Async/Await Issues

**Issue**: `error: no 'async' modifier on 'func'`

**Solution - Mark async**:
```swift
// ✅ Correct
async func executeQuery(_ query: String) throws -> QueryResult {
    let result = try await fraiseql.execute(query)
    return result
}
```

---

### Runtime Errors

#### URLSession Issues

**Issue**: `error: The network connection was lost`

**Solution - Handle network errors**:
```swift
// ✅ Handle errors
do {
    let result = try await fraiseql.execute(query)
    return result
} catch let error as URLError {
    print("Network error: \(error.localizedDescription)")
} catch {
    print("Other error: \(error)")
}
```

#### Decoding Errors

**Issue**: `DecodingError.dataCorrupted`

**Solution - Debug decoding**:
```swift
// ✅ Use custom decoder
let decoder = JSONDecoder()
decoder.dateDecodingStrategy = .iso8601

do {
    let result = try decoder.decode(QueryResult.self, from: data)
} catch {
    print("Decode error: \(error)")
}
```

#### Thread Safety Issues

**Issue**: `data races detected`

**Solution - Use MainActor**:
```swift
// ✅ Main thread only
@MainActor
func updateUI(_ result: QueryResult) {
    // Safe to update UI
}

// ✅ Or use nonisolated
nonisolated func backgroundTask() {
    // Not on main thread
}
```

#### Memory Issues

**Issue**: `EXC_BAD_ACCESS` or memory warnings

**Solution - Manage resources**:
```swift
// ✅ Auto-cleanup
class MyService {
    var fraiseql: FraiseQLServer?

    deinit {
        fraiseql = nil  // Cleanup
    }
}

// ✅ Or use weak references
weak var server: FraiseQLServer?
```

---

### Performance Issues

#### Build Time

**Issue**: Build takes >2 minutes

**Parallel compilation**:
```bash
swift build -c release -Xswiftc -g0
```

**Incremental builds**:
```bash
swift build -v  # Verbose to see incremental changes
```

#### App Size

**Issue**: Binary is >100MB

**Optimize with -Onone for development**:
```bash
swift build -c release -Xswiftc -Onone
```

**Or full optimization**:
```bash
swift build -c release -Xswiftc -Osize
```

#### Memory Usage

**Issue**: iOS app uses >200MB

**Profile with Instruments**:
1. Xcode → Product → Profile (Cmd+I)
2. Select Memory profiler
3. Run app and inspect

**Optimize**:
- Use lazy sequences
- Release large objects
- Implement custom Codable for efficiency

#### Network Performance

**Issue**: Queries timeout or are slow

**Increase timeout**:
```swift
var request = URLRequest(url: graphqlURL, cachePolicy: .useProtocolCachePolicy, timeoutInterval: 60)
let session = URLSession(configuration: .default)
let data = try await session.data(for: request)
```

---

### Debugging Techniques

#### Print Debugging

```swift
print("Query: \(query)")
print("Result: \(result)")
debugPrint("Detailed: \(result)")
```

#### Xcode Debugger

1. Set breakpoint (Cmd+B on line)
2. Run with debug (Cmd+R)
3. Step through (F6)
4. Inspect in Variables panel

#### Logging

```swift
import os

let logger = Logger()

logger.debug("Query: \(query)")
logger.info("Execution started")
logger.error("Query failed: \(error)")
```

**View logs**:
```bash
log stream --predicate 'eventMessage contains "Query"'
```

#### Unit Tests

```swift
import XCTest

class FraiseQLTests: XCTestCase {
    func testQuery() async throws {
        let server = try FraiseQLServer.fromCompiled("schema.compiled.json")
        let result = try await server.execute("{ user(id: 1) { id } }")
        XCTAssertNotNil(result)
    }
}
```

---

### Getting Help

#### GitHub Issues

Provide:
1. Swift version: `swift --version`
2. macOS/iOS version
3. Xcode version
4. FraiseQL version
5. Minimal reproducible example
6. Full error message

**Template**:
```markdown
**Environment**:
- Swift: 5.9
- iOS: 16.0 / macOS: 13.0
- Xcode: 15.0
- FraiseQL: 2.0.0

**Issue**:
[Describe]

**Code**:
[Minimal example]

**Error**:
[Full error message]
```

#### Community Channels

- **GitHub Discussions**: Q&A
- **Swift Forum**: https://forums.swift.org
- **Stack Overflow**: Tag with `swift` and `fraiseql`

#### Profiling Tools

**Instruments** (in Xcode):
1. Product → Profile (Cmd+I)
2. Select Core Data / Memory / Network
3. Run and analyze

---

## See Also

- [FraiseQL Swift SDK on GitHub](https://github.com/fraiseql/fraiseql-swift)

---

**Remember:** Swift is for schema authoring only. The Rust compiler transforms your schema into optimized SQL. Build once, deploy to FraiseQL server, run zero-cost native operations across iOS, macOS, and server environments.
