<!-- Skip to main content -->
---
title: FraiseQL Scala SDK Reference
description: Complete API reference for the FraiseQL Scala SDK. This guide covers the complete Scala authoring interface for building type-safe GraphQL APIs with functional 
keywords: ["framework", "directives", "types", "sdk", "schema", "scalars", "monitoring", "api"]
tags: ["documentation", "reference"]
---

# FraiseQL Scala SDK Reference

**Status**: Production-Ready | **Scala Version**: 3.3+ | **SDK Version**: 2.0.0+
**Last Updated**: 2026-02-05 | **Maintained By**: FraiseQL Community

Complete API reference for the FraiseQL Scala SDK. This guide covers the complete Scala authoring interface for building type-safe GraphQL APIs with functional programming patterns, case classes, sealed traits, and compile-time schema generation. Scala is a pure authoring language—no runtime FFI or native bindings required.

## Installation & Setup

### SBT Configuration

Add FraiseQL to your `build.sbt`:

```scala
<!-- Code example in SCALA -->
val scala3Version = "3.3.1"

ThisBuild / scalaVersion := scala3Version

libraryDependencies ++= Seq(
  "com.FraiseQL" %% "FraiseQL-core" % "2.0.0",
  "com.FraiseQL" %% "FraiseQL-schema" % "2.0.0",
  // For functional effects (optional, but recommended)
  "org.typelevel" %% "cats-core" % "2.10.0",
  "org.typelevel" %% "cats-effect" % "3.5.0",
  // Testing support
  "org.scalactic" %% "scalactic" % "3.2.17" % Test,
  "org.scalatest" %% "scalatest" % "3.2.17" % Test,
)

scalacOptions ++= Seq(
  "-encoding", "UTF-8",
  "-deprecation",
  "-feature",
  "-unchecked",
  "-Wunused:imports",
  "-Xfatal-warnings",
)
```text
<!-- Code example in TEXT -->

### Requirements

- **Scala 3.3+** (Full support, all features)
- **Scala 2.13** (Limited support, macros required)
- **SBT 1.9.0+** or **Mill 0.11+**
- **JVM 11+** (LTS versions: 11, 17, 21 recommended)
- Optional: Cats library for functional effect patterns

### First Schema (90 seconds)

```scala
<!-- Code example in SCALA -->
import com.FraiseQL.schema.*
import com.FraiseQL.schema.dsl.*

@Type("user")
case class User(
  id: Int,
  name: String,
  email: Option[String] = None,
  isActive: Boolean = true,
)

object UserSchema:
  val userQuery = query("user")
    .returnType[User]
    .arg("id", GraphQLInt)
    .description("Fetch user by ID")

  val usersQuery = query("users")
    .returnType[List[User]]
    .arg("limit", GraphQLInt)
    .arg("offset", GraphQLInt)
    .description("Fetch paginated users")

  val schema = FraiseQL.schema
    .registerType[User]
    .registerQuery(userQuery)
    .registerQuery(usersQuery)
    .exportToFile("schema.json")

@main def generateSchema(): Unit =
  UserSchema.schema
  println("Schema exported to schema.json")
```text
<!-- Code example in TEXT -->

Export and deploy to FraiseQL server:

```bash
<!-- Code example in BASH -->
sbt run
FraiseQL-cli compile schema.json FraiseQL.toml
FraiseQL-server --schema schema.compiled.json
```text
<!-- Code example in TEXT -->

---

## Quick Reference Table

| Feature | Construct | Purpose | Pattern |
|---------|-----------|---------|---------|
| **Type Definition** | `case class` with `@Type` | GraphQL object type | Immutable data |
| **Field Definition** | `class constructor arg` | Type field | Strongly typed |
| **Query Operation** | `query()` builder | Read operation (SELECT) | Functional chain |
| **Mutation Operation** | `mutation()` builder | Write operation (CREATE/UPDATE/DELETE) | Functional chain |
| **Sealed Hierarchy** | `sealed trait` + `case class` | Polymorphic types (Union) | Pattern matching |
| **Fact Table** | `@FactTable` on case class | Analytics dimension/measure | OLAP structure |
| **RBAC/Security** | `@Secured` annotation | Access control directive | Role-based |
| **Field Metadata** | `@Deprecated`, `@Field` | Schema metadata | Compile-time |
| **Input Type** | `case class` with `@Input` | Mutation arguments | Immutable builder |
| **Enum Type** | `sealed trait` + `case object` | GraphQL enum | Type-safe |

---

## Type System: Functional Foundations

### 1. Case Classes as GraphQL Types

Scala case classes provide ideal foundations for GraphQL types—immutable, automatically generated equality, copy semantics, and pattern matching support.

```scala
<!-- Code example in SCALA -->
// Simple type with required fields
@Type("user")
case class User(
  id: Int,
  name: String,
)

// Type with optional fields (None = nullable in GraphQL)
@Type("product")
case class Product(
  id: Int,
  name: String,
  description: Option[String] = None,
  price: BigDecimal,
  inStock: Boolean = true,
)

// Type with list fields
@Type("order")
case class Order(
  id: Int,
  userId: Int,
  items: List[OrderItem],
  total: BigDecimal,
  createdAt: java.time.Instant,
)

// Nested case classes (automatically flattened or referenced)
@Type("orderItem")
case class OrderItem(
  productId: Int,
  quantity: Int,
  unitPrice: BigDecimal,
)
```text
<!-- Code example in TEXT -->

**Type Mapping Rules:**

| Scala Type | GraphQL Type | Nullable | Example |
|-----------|--------------|----------|---------|
| `Int`, `Long`, `Short`, `Byte` | `Int!` | No | `case class Qty(count: Int)` |
| `Option[Int]` | `Int` | Yes | `id: Option[Int] = None` |
| `BigDecimal`, `Double` | `Float!` | No | `price: BigDecimal` |
| `String` | `String!` | No | `name: String` |
| `Boolean` | `Boolean!` | No | `isActive: Boolean` |
| `java.time.Instant` | `DateTime!` | No | `createdAt: Instant` |
| `java.util.UUID` | `ID!` | No | `uuid: UUID` |
| `List[T]` | `[T]!` | No | `items: List[Item]` |
| `Option[List[T]]` | `[T]` | Yes | `tags: Option[List[String]]` |

### 2. Sealed Trait Hierarchies (Unions)

Use sealed traits with case classes to model GraphQL union types and polymorphic queries.

```scala
<!-- Code example in SCALA -->
// Union type: SearchResult = User | Product | Article
sealed trait SearchResult

@Type("user")
case class User(id: Int, name: String) extends SearchResult

@Type("product")
case class Product(id: Int, title: String, price: BigDecimal)
  extends SearchResult

@Type("article")
case class Article(id: Int, title: String, author: String)
  extends SearchResult

// Pattern matching on union results
def formatSearchResult(result: SearchResult): String =
  result match
    case User(id, name) => s"User: $name (#$id)"
    case Product(id, title, price) => s"Product: $title ($price)"
    case Article(id, title, author) => s"Article: $title by $author"
```text
<!-- Code example in TEXT -->

### 3. Type Aliases and Opaque Types

Use type aliases for domain-specific types and compile-time safety:

```scala
<!-- Code example in SCALA -->
object Types:
  type UserId = Int
  opaque type Email = String
  opaque type Slug = String

  def email(value: String): Email = value
  def slug(value: String): Slug = value

@Type("user")
case class User(
  id: Types.UserId,
  email: Types.Email,
  slug: Types.Slug,
)
```text
<!-- Code example in TEXT -->

---

## Operations: Functional Query Builder

### Query Operations (Read)

Define read-only operations using functional composition:

```scala
<!-- Code example in SCALA -->
object UserQueries:
  // Simple single-result query
  val getUser = query("user")
    .returnType[User]
    .arg("id", GraphQLInt)
    .description("Fetch user by ID")

  // List query with pagination
  val listUsers = query("users")
    .returnType[List[User]]
    .arg("limit", GraphQLInt)
    .arg("offset", GraphQLInt)
    .description("Paginated user list")

  // Query with filtering using sealed traits
  sealed trait UserFilter
  case class ByRole(role: String) extends UserFilter
  case class ByStatus(active: Boolean) extends UserFilter

  val filteredUsers = query("filteredUsers")
    .returnType[List[User]]
    .arg("filter", UserFilter)
    .description("Users by filter criteria")
```text
<!-- Code example in TEXT -->

### Mutation Operations (Write)

Define write operations with input types and error handling:

```scala
<!-- Code example in SCALA -->
@Input("createUserInput")
case class CreateUserInput(
  name: String,
  email: String,
  role: Option[String] = None,
)

@Input("updateUserInput")
case class UpdateUserInput(
  id: Int,
  name: Option[String] = None,
  email: Option[String] = None,
)

sealed trait MutationResult
case class CreateUserSuccess(user: User) extends MutationResult
case class CreateUserError(message: String) extends MutationResult

object UserMutations:
  val createUser = mutation("createUser")
    .returnType[MutationResult]
    .arg("input", CreateUserInput)
    .description("Create new user")

  val updateUser = mutation("updateUser")
    .returnType[MutationResult]
    .arg("input", UpdateUserInput)
    .description("Update user")

  val deleteUser = mutation("deleteUser")
    .returnType[Boolean]
    .arg("id", GraphQLInt)
    .description("Delete user by ID")
```text
<!-- Code example in TEXT -->

---

## Advanced Features: RBAC & Analytics

### RBAC with Security Traits

Compose security constraints using functional patterns:

```scala
<!-- Code example in SCALA -->
@Secured(roles = List("admin", "user_manager"))
val adminUsers = query("adminUsers")
  .returnType[List[User]]
  .description("List all users (admin only)")

// Composable security predicates
sealed trait SecurityContext
case class UserContext(userId: Int, roles: Set[String])
  extends SecurityContext

def requireRole(role: String)(ctx: SecurityContext): Boolean =
  ctx match
    case UserContext(_, roles) => roles.contains(role)
    case _ => false
```text
<!-- Code example in TEXT -->

### Fact Tables for Analytics

Define analytics tables with measures and dimensions:

```scala
<!-- Code example in SCALA -->
@FactTable("sales_fact")
case class SalesFact(
  // Dimensions (categorical attributes)
  @Dimension dateId: Int,
  @Dimension productId: Int,
  @Dimension regionId: Int,
  // Measures (numeric aggregates)
  @Measure revenue: BigDecimal,
  @Measure quantity: Int,
  @Measure discount: Option[BigDecimal] = None,
)

// Aggregate query example
val salesByRegion = aggregateQuery("salesByRegion")
  .table[SalesFact]
  .dimensions(List("regionId"))
  .measures(List("revenue", "quantity"))
  .description("Total sales revenue and quantity by region")
```text
<!-- Code example in TEXT -->

### Field-Level Metadata

Annotate fields with metadata for schema documentation:

```scala
<!-- Code example in SCALA -->
case class User(
  @Field(description = "User's primary key", required = true)
  id: Int,

  @Deprecated(reason = "Use fullName instead", since = "2.0.0")
  name: String,

  @Field(requiresScope = "user:email")
  email: String,

  @Field(example = "john_doe")
  username: String,
)
```text
<!-- Code example in TEXT -->

---

## Scalar Types: Scala ↔ GraphQL Mapping

FraiseQL automatically maps Scala scalar types to GraphQL scalars:

```scala
<!-- Code example in SCALA -->
object GraphQLScalars:
  // Numeric types
  Int → GraphQL Int
  Long → GraphQL Int (overflow possible, recommend BigInt)
  BigDecimal → GraphQL Float or Decimal
  Double → GraphQL Float (precision loss possible)
  Float → GraphQL Float

  // Text types
  String → GraphQL String
  Char → GraphQL String (single character)

  // Boolean
  Boolean → GraphQL Boolean

  // Temporal types
  java.time.Instant → GraphQL DateTime
  java.time.LocalDate → GraphQL Date
  java.time.LocalDateTime → GraphQL DateTime
  java.time.ZonedDateTime → GraphQL DateTime

  // Identifiers
  java.util.UUID → GraphQL ID
  Option[T] → GraphQL (nullable)

  // Collections
  List[T] → GraphQL [T]!
  Vector[T] → GraphQL [T]!
  Set[T] → GraphQL [T]!

// Custom scalar example
case class JSON(value: String)
case class Decimal128(value: String) // MongoDB extended JSON
```text
<!-- Code example in TEXT -->

---

## Schema Export Workflow

### Compilation Pipeline

```scala
<!-- Code example in SCALA -->
object SchemaBuilder:
  def exportSchema(): Unit =
    FraiseQL.schema
      .registerType[User]
      .registerType[Product]
      .registerType[Order]
      .registerQuery(UserQueries.getUser)
      .registerQuery(UserQueries.listUsers)
      .registerMutation(UserMutations.createUser)
      .registerMutation(UserMutations.updateUser)
      .exportToFile("schema.json")

@main def generateSchema(): Unit =
  SchemaBuilder.exportSchema()
  println("✓ Schema exported to schema.json")
  println("✓ Run: FraiseQL-cli compile schema.json FraiseQL.toml")
```text
<!-- Code example in TEXT -->

### SBT Tasks

```bash
<!-- Code example in BASH -->
# Generate schema.json
sbt "runMain SchemaBuilder"

# Compile schema to schema.compiled.json
FraiseQL-cli compile schema.json FraiseQL.toml

# Validate compiled schema
FraiseQL-cli validate schema.compiled.json

# Serve with FraiseQL runtime
FraiseQL-server --schema schema.compiled.json --bind 0.0.0.0:8080
```text
<!-- Code example in TEXT -->

---

## Type Mapping Reference

### Automatic Type Inference

FraiseQL infers GraphQL types from Scala case class definitions:

```scala
<!-- Code example in SCALA -->
// Case class definition
@Type("user")
case class User(
  id: Int,                              // → Int!
  email: String,                        // → String!
  age: Option[Int] = None,              // → Int
  tags: List[String] = List(),          // → [String]!
  metadata: Option[Map[String, String]] // → [String]!
)

// Exported as:
/*
type User {
  id: Int!
  email: String!
  age: Int
  tags: [String]!
  metadata: [String]!
}
*/
```text
<!-- Code example in TEXT -->

---

## Common Functional Patterns

### CRUD with For-Comprehensions

Use for-comprehensions for composable query chains:

```scala
<!-- Code example in SCALA -->
case class UserService(queries: UserQueries):
  // Functional composition using for-comprehension
  def fetchUserWithPosts(userId: Int): Option[(User, List[Post])] =
    for
      user <- queries.getUser(userId)
      posts <- queries.getUserPosts(userId)
    yield (user, posts)

  // Error handling with Either
  def createUserValidated(
    input: CreateUserInput
  ): Either[ValidationError, User] =
    for
      _ <- validateEmail(input.email)
      _ <- validateName(input.name)
      user <- queries.createUser(input)
    yield user
```text
<!-- Code example in TEXT -->

### Pattern Matching on Results

```scala
<!-- Code example in SCALA -->
def displayUser(result: Either[Error, User]): String =
  result match
    case Right(user) => s"✓ User: ${user.name}"
    case Left(err) => s"✗ Error: ${err.message}"

def processSearchResults(results: List[SearchResult]): Unit =
  results.foreach {
    case User(id, name) => println(s"User: $name")
    case Product(id, title, _) => println(s"Product: $title")
    case Article(id, title, _) => println(s"Article: $title")
  }
```text
<!-- Code example in TEXT -->

### Implicit Conversions and Typeclass Patterns

```scala
<!-- Code example in SCALA -->
// Typeclass for GraphQL serialization
trait GraphQLSerializable[T]:
  def toGraphQL(): String

given GraphQLSerializable[User] with
  def toGraphQL(user: User): String =
    s"""{ id: ${user.id}, name: "${user.name}" }"""

// Implicit extension methods
extension [T: GraphQLSerializable]()
  def toGraphQL: String = summon[GraphQLSerializable[T]].toGraphQL(value)
```text
<!-- Code example in TEXT -->

---

## Error Handling Patterns

### Option for Nullable Values

```scala
<!-- Code example in SCALA -->
def findUser(id: Int): Option[User] =
  // Returns Option, maps to nullable GraphQL type
  if id > 0 then Some(User(id, "John")) else None

// Use map/flatMap for functional chains
val userEmail: Option[String] =
  findUser(1).map(_.email)
```text
<!-- Code example in TEXT -->

### Either for Result Types

```scala
<!-- Code example in SCALA -->
type Result[T] = Either[FraiseQLError, T]

sealed trait FraiseQLError:
  def message: String

case class ValidationError(message: String) extends FraiseQLError
case class DatabaseError(message: String) extends FraiseQLError

def createUser(input: CreateUserInput): Result[User] =
  for
    _ <- validateInput(input)
    user <- saveUser(input)
  yield user
```text
<!-- Code example in TEXT -->

### Try for Exception Handling

```scala
<!-- Code example in SCALA -->
import scala.util.{Try, Success, Failure}

def parseConfig(json: String): Try[Config] =
  Try(Json.parse(json).as[Config])

def loadSchema(path: String): Try[Schema] =
  for
    content <- Try(scala.io.Source.fromFile(path).mkString)
    config <- parseConfig(content)
  yield config
```text
<!-- Code example in TEXT -->

---

## Testing Patterns

### ScalaTest for Unit Tests

```scala
<!-- Code example in SCALA -->
class UserQueriesSpec extends AnyFlatSpec with Matchers:
  "UserQueries.getUser" should "return user by ID" in {
    val user = UserQueries.getUser(1)
    user should not be empty
    user.get.id should equal(1)
  }

  it should "return None for invalid ID" in {
    val user = UserQueries.getUser(-1)
    user should be(None)
  }

class UserMutationsSpec extends AsyncFlatSpec with Matchers:
  "UserMutations.createUser" should "create user with valid input" in {
    val input = CreateUserInput("John", "john@example.com")
    UserMutations.createUser(input).map { result =>
      result should matchPattern { case CreateUserSuccess(_) => }
    }
  }
```text
<!-- Code example in TEXT -->

### Property-Based Testing with ScalaCheck

```scala
<!-- Code example in SCALA -->
import org.scalacheck.Gen
import org.scalacheck.Prop.forAll

property("User ID should always be positive") = forAll {
  (id: Int) => id > 0 ==> {
    val user = User(id, "Test")
    user.id > 0
  }
}
```text
<!-- Code example in TEXT -->

---

## See Also

- **[Python Reference](./python-reference.md)** — Python authoring with decorators
- **[Java Reference](./java-reference.md)** — Java authoring with annotations
- **[TypeScript Reference](./typescript-reference.md)** — TypeScript authoring with decorators
- **[RBAC Guide](../../enterpri../../guides/authorization-quick-start.md)** — Role-based access control patterns

---

## Troubleshooting

### Common Setup Issues

#### SBT Dependency Issues

**Issue**: `not found: SbtModule: FraiseQL`

**Solution**:

```scala
<!-- Code example in SCALA -->
// build.sbt
libraryDependencies += "com.FraiseQL" %% "FraiseQL-scala" % "2.0.0"

// Or with additional options
libraryDependencies ++= Seq(
  "com.FraiseQL" %% "FraiseQL-scala" % "2.0.0",
  "org.scala-lang" % "scala-library" % scalaVersion.value
)
```text
<!-- Code example in TEXT -->

```bash
<!-- Code example in BASH -->
sbt clean update
```text
<!-- Code example in TEXT -->

#### Compilation Errors

**Issue**: `[error] could not find implicit value for parameter`

**Cause**: Missing implicits

**Solution**:

```scala
<!-- Code example in SCALA -->
// Import required implicits
import com.FraiseQL._
import com.FraiseQL.Implicits._

// Or in object
object MyApp {
  import com.FraiseQL._

  def main(args: Array[String]): Unit = {
    // Now implicits available
  }
}
```text
<!-- Code example in TEXT -->

#### Type Inference Issues

**Issue**: `type mismatch; found: String, required: FraiseQL.String`

**Solution - Use correct types**:

```scala
<!-- Code example in SCALA -->
// ✅ Correct
@FraiseQL.type
case class User(
  id: Int,
  email: String
)

// Or use type aliases
type Email = String
@FraiseQL.type
case class User(email: Email)
```text
<!-- Code example in TEXT -->

#### Scala Version Mismatch

**Issue**: `java.lang.NoClassDefFoundError`

**Check version** (2.13+ required):

```bash
<!-- Code example in BASH -->
scala -version
```text
<!-- Code example in TEXT -->

**Set in build.sbt**:

```scala
<!-- Code example in SCALA -->
scalaVersion := "2.13.11"
scalacOptions ++= Seq("-feature", "-deprecation")
```text
<!-- Code example in TEXT -->

---

### Type System Issues

#### Pattern Matching Issues

**Issue**: `non-exhaustive pattern match`

**Solution - Complete patterns**:

```scala
<!-- Code example in SCALA -->
// ❌ Incomplete
val user = getUserOption()
val name = user match {
  case Some(u) => u.name
  // case None missing!
}

// ✅ Complete
val name = user match {
  case Some(u) => u.name
  case None => "Unknown"
}
```text
<!-- Code example in TEXT -->

#### Implicit Resolution

**Issue**: `could not find implicit value for parameter`

**Solution - Define implicits**:

```scala
<!-- Code example in SCALA -->
implicit val config: FraiseQLConfig = FraiseQLConfig.default

// Or scope
object FraiseQL {
  implicit val config: FraiseQLConfig = FraiseQLConfig.default

  def execute(query: String)(implicit c: FraiseQLConfig) = {
    // Use c
  }
}
```text
<!-- Code example in TEXT -->

#### Type Class Issues

**Issue**: `value json is not a member of MyType`

**Solution - Implement typeclass**:

```scala
<!-- Code example in SCALA -->
import com.FraiseQL.Serializable

implicit object MyTypeSerializable extends Serializable[MyType] {
  def toJson(value: MyType): String = {
    // Serialize to JSON
    ""
  }
}
```text
<!-- Code example in TEXT -->

#### Higher-Kinded Type Errors

**Issue**: `[error] FraiseQL[T] is not a type constructor with expected kind [error]`

**Solution - Use correct kind**:

```scala
<!-- Code example in SCALA -->
// ❌ Wrong - treating as type
val result: FraiseQL[User] = query()

// ✅ Correct - use concrete type
val result: FraiseQL.Result[User] = query()
```text
<!-- Code example in TEXT -->

---

### Runtime Errors

#### Match Error

**Issue**: `scala.MatchError: ...`

**Solution - Handle all cases**:

```scala
<!-- Code example in SCALA -->
val result = server.execute(query)

result match {
  case r: QueryResult => r.data
  case e: ExecutionError => e.message
  case _ => "Unknown result"
}
```text
<!-- Code example in TEXT -->

#### Null Pointer Exception

**Issue**: `NullPointerException`

**Solution - Use Option**:

```scala
<!-- Code example in SCALA -->
// ❌ Can NPE
val user = getUser()
println(user.name)  // NPE if null

// ✅ Safe with Option
val user = getUser()
user.foreach(u => println(u.name))

// Or match
user match {
  case Some(u) => println(u.name)
  case None => println("Not found")
}
```text
<!-- Code example in TEXT -->

#### Future/Promise Issues

**Issue**: `NoSuchElementException: Future.get on failed Future`

**Solution - Handle Future correctly**:

```scala
<!-- Code example in SCALA -->
import scala.concurrent._
import scala.util.{Success, Failure}

val future = server.executeAsync(query)

future.onComplete {
  case Success(result) => println(result)
  case Failure(error) => println(s"Error: $error")
}

// Or use map/flatMap
future.map(result => process(result))
      .recover { case e => handleError(e) }
```text
<!-- Code example in TEXT -->

#### Actor Timeout

**Issue**: `AskTimeoutException`

**Solution - Increase timeout**:

```scala
<!-- Code example in SCALA -->
import scala.concurrent.duration._

implicit val timeout: Timeout = Timeout(30.seconds)

val result = server.ask(ExecuteQuery(query))
```text
<!-- Code example in TEXT -->

---

### Performance Issues

#### Compilation Slowdown

**Issue**: Build takes >60 seconds

**Enable incremental compilation**:

```scala
<!-- Code example in SCALA -->
// build.sbt
incOptions := incOptions.value.withRecompileOnMacroDef(false)
```text
<!-- Code example in TEXT -->

**Parallel execution**:

```bash
<!-- Code example in BASH -->
sbt -J-Xmx2g -J-XX:+UseG1GC
sbt parallelExecution in Test := true
```text
<!-- Code example in TEXT -->

#### Memory Issues

**Issue**: `OutOfMemoryError: Java heap space`

**Increase heap**:

```bash
<!-- Code example in BASH -->
sbt -J-Xmx4g -J-Xms2g
```text
<!-- Code example in TEXT -->

**Or in build.sbt**:

```scala
<!-- Code example in SCALA -->
javaOptions ++= Seq("-Xmx4g", "-Xms2g")
```text
<!-- Code example in TEXT -->

#### Lazy Evaluation Issues

**Issue**: `StackOverflowError` with recursive lazy values

**Solution - Use streams carefully**:

```scala
<!-- Code example in SCALA -->
// ❌ Can overflow
lazy val infinite: Stream[Int] = 1 #:: infinite.map(_ + 1)

// ✅ Use Iterator or LazyList
lazy val lazy_list = LazyList.from(1)
```text
<!-- Code example in TEXT -->

---

### Debugging Techniques

#### Enable Logging

**Setup logging**:

```scala
<!-- Code example in SCALA -->
import org.slf4j.LoggerFactory

val logger = LoggerFactory.getLogger(getClass)

logger.debug("Executing query: {}", query)
val result = server.execute(query)
logger.info("Result: {}", result)
```text
<!-- Code example in TEXT -->

**Set log level**:

```bash
<!-- Code example in BASH -->
RUST_LOG=FraiseQL=debug sbt run
```text
<!-- Code example in TEXT -->

#### REPL Debugging

**Use Scala REPL**:

```bash
<!-- Code example in BASH -->
sbt console
```text
<!-- Code example in TEXT -->

```scala
<!-- Code example in SCALA -->
scala> import com.FraiseQL._
scala> val server = Server.fromCompiled("schema.json")
scala> server.execute("{ user(id: 1) { id } }")
```text
<!-- Code example in TEXT -->

#### Pattern Match Debugging

```scala
<!-- Code example in SCALA -->
val result = server.execute(query)

result match {
  case r @ QueryResult(data, _) =>
    println(s"Data: $data")
  case e @ ExecutionError(msg, _) =>
    println(s"Error: $msg")
  case other =>
    println(s"Unexpected: $other")
}
```text
<!-- Code example in TEXT -->

#### Property Testing

```scala
<!-- Code example in SCALA -->
import org.scalacheck.Properties

object QueryProperties extends Properties("Query") {
  property("result is non-empty") = forAll { (query: String) =>
    server.execute(query).data.nonEmpty
  }
}
```text
<!-- Code example in TEXT -->

---

### Getting Help

#### GitHub Issues

Provide:

1. Scala version: `scala -version`
2. Java version: `java -version`
3. SBT version: `sbt sbtVersion`
4. FraiseQL version
5. Minimal reproducible example

**Template**:

```markdown
<!-- Code example in MARKDOWN -->
**Environment**:
- Scala: 2.13.11
- Java: 11
- FraiseQL: 2.0.0

**Issue**:
[Describe]

**Code**:
[Minimal example]

**Error**:
[Full stack trace]
```text
<!-- Code example in TEXT -->

#### Community Channels

- **Scala Community**: <https://contributors.scala-lang.org/>
- **Stack Overflow**: Tag with `scala` and `FraiseQL`
- **GitHub Discussions**: Q&A

---

## See Also

- **[Fact Tables Guide](../../architecture/analytics/fact-dimension-pattern.md)** — Analytics dimension modeling
- **[Schema Validation](../../guides/README.md)** — Compile-time schema validation
- **[CLI Reference](../../reference/cli-commands-cheatsheet.md)** — `FraiseQL-cli` commands and options
- **[Cats Library](https://typelevel.org/cats/)** — Functional effect composition patterns
- **[Scala Documentation](https://docs.scala-lang.org/)** — Official Scala language reference
