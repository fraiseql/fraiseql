## Comprehensive List of Strawberry GraphQL Functionalities Used in printoptim_backend

Based on my analysis of the printoptim_backend project, here's a comprehensive list of all Strawberry GraphQL functionalities being used:

### 1. **Decorators Used**

#### `@strawberry.type`
- Used extensively for defining GraphQL object types
- Examples:
  ```python
  @strawberry.type
  class Manufacturer(BaseGQLType):
      id: uuid.UUID
      identifier: str
      name: str
  ```

#### `@strawberry.input`
- Used for defining GraphQL input types for mutations
- Example:
  ```python
  @strawberry.input
  class CreateModelInput:
      manufacturer_id: uuid.UUID
      generic_model_id: uuid.UUID
      name: str
  ```

#### `@strawberry.field`
- Used to define GraphQL query fields within query classes
- Example:
  ```python
  @strawberry.field
  async def manufacturer(self, info: Info, manufacturer_id: uuid.UUID, repo: Annotated[PsycopgRepository, Depends(get_repository)]) -> Manufacturer | None:
  ```

#### `@strawberry.mutation`
- Used to define GraphQL mutation fields
- Example:
  ```python
  @strawberry.mutation
  def create_model(self, info: Info, input_: CreateModelInput) -> CreateModelResult:
  ```

#### `@strawberry.interface`
- Used to define GraphQL interfaces
- Example:
  ```python
  @strawberry.interface
  class BaseContract:
      id: uuid.UUID
      identifier: str
      signature_date: date | None
  ```

#### `@strawberry.enum`
- Used to define GraphQL enums
- Examples:
  ```python
  @strawberry.enum
  class AccessoryType(Enum):
      PRODUCT = "product"
      ITEM = "item"

  @strawberry.enum
  class OrderByDirection(enum.Enum):
      ASC = "ASC"
      DESC = "DESC"
  ```

#### `@strawberry.scalar`
- Used to define custom scalar types
- Examples:
  ```python
  @strawberry.scalar
  class IsoDate:
      @staticmethod
      def serialize(value: date) -> str:
          return value.isoformat()

  @strawberry.scalar
  class IpAddressString:
      @staticmethod
      def serialize(value: IPv4Address | IPv6Address) -> str:
          return str(value)
  ```

### 2. **Type System Features**

#### **Union Types**
- Extensively used for mutation results (Success | Error pattern)
- Example:
  ```python
  CreateModelResult = Annotated[
      CreateModelError | CreateModelSuccess,
      strawberry.union("CreateModelResult"),
  ]
  ```

#### **Interface Types**
- Used for shared base functionality
- Example: `BaseContract` interface implemented by `Contract` and `ContractWithPriceList`

#### **Generic Types**
- Used for pagination responses
- Example:
  ```python
  @strawberry.type
  class PaginatedResponse(Generic[T]):
      data: list[T]
      total_count: int
  ```

#### **Custom Scalar Types**
- `IsoDate` - ISO-formatted date strings
- `IpAddressString` - IPv4 and IPv6 addresses
- `SubnetMaskString` - Subnet mask strings
- `DateRange` - Date range values
- `LTreePath` - PostgreSQL ltree hierarchical paths

### 3. **Field Resolver Patterns**

#### **Async Field Resolvers**
- All query resolvers are async functions
- Example:
  ```python
  @strawberry.field
  async def manufacturers(self, info: Info, repo: ..., where: ManufacturerWhere | None = None) -> list[Manufacturer]:
  ```

#### **Dependency Injection**
- Uses FastAPI's `Depends` for repository injection
- Example:
  ```python
  repo: Annotated[PsycopgRepository, Depends(get_repository)]
  ```

### 4. **Context Usage Patterns**

#### **Tenant ID from Context**
- Multi-tenancy support via context
- Example:
  ```python
  tenant_id=info.context["tenant_id"]
  ```

#### **User Information from Context**
- Authentication info passed through context
- Example in `run_sql_mutation.py`:
  ```python
  user_id = info.context.get("user_id")
  customer_org_id = info.context.get("customer_org_id")
  ```

### 5. **Permission/Authorization Patterns**

While no direct Strawberry permission decorators are used, authorization is handled through:
- Context-based user/tenant filtering
- Basic auth for development endpoints
- Auth0 integration for production

### 6. **Pagination Patterns**

#### **OrderBy Instructions**
```python
@strawberry.input
class OrderByInstruction:
    field: str
    direction: OrderByDirection

@strawberry.input
class OrderByInstructions:
    instructions: list[OrderByInstruction]
```

#### **Pagination Input**
```python
@strawberry.input
class PaginationInput:
    limit: int | None = 250
    offset: int | None = 0
```

#### **Paginated Response**
```python
@strawberry.type
class PaginatedResponse(Generic[T]):
    data: list[T]
    total_count: int
```

### 7. **Error Handling Patterns**

#### **Structured Error Type**
```python
@strawberry.type
class Error:
    message: str
    code: int
    identifier: str
    details: JSONType | None = None
```

#### **Mutation Result Base**
```python
@strawberry.type
class MutationResultBase:
    id_: uuid.UUID | None = strawberry.field(default=None)
    updated_fields: list[str] | None = strawberry.field(default=None)
    status: str
    message: str | None = strawberry.field(default=None)
    metadata: JSONType | None = strawberry.field(default=None)
    errors: list[Error] | None = strawberry.field(default=None)
```

#### **Union-based Error Handling**
- Every mutation returns a union of Success | Error types
- Status mapping for business rule violations

### 8. **Other Strawberry-Specific Features**

#### **Schema Assembly**
```python
schema = strawberry.Schema(
    query=RootQuery,
    mutation=RootMutation,
    scalar_overrides={
        datetime.date: IsoDate,
    },
)
```

#### **merge_types Utility**
- Used to combine multiple query/mutation types
```python
RootQuery = merge_types("RootQuery", QUERY_TYPES)
RootMutation = merge_types("RootMutation", MUTATION_TYPES)
```

#### **GraphQL Router Integration**
```python
GraphQLRouter(
    schema.schema,
    path="/graphql",
    context_getter=auth_utils.get_context,
    graphql_ide="graphiql",
)
```

#### **JSON Scalar**
- Used extensively for metadata and flexible data structures
```python
from strawberry.scalars import JSON
```

#### **Field Defaults**
```python
currency: str | None = "EUR"
limit: int | None = 250
```

#### **Field Descriptions**
```python
manufacturer_accessory_id: uuid.UUID = strawberry.field(
    description="The ManufacturerAccessoryId.",
)
```

#### **Type Annotations with strawberry.field**
- Used for customizing field behavior
```python
id_: uuid.UUID | None = strawberry.field(default=None)
```

### 9. **Custom Utilities Built on Strawberry**

#### **BaseGQLType**
- Custom base class for GraphQL types with nested field parsing
- Provides `from_dict` method for instantiation from database results

#### **Where Type Generator**
- `safe_create_where_type` utility for creating filter types
```python
ManufacturerWhere = safe_create_where_type(Manufacturer)
```

#### **Logging Decorator**
- Custom decorator for logging resolver calls
```python
@log_resolver(app_logger)
```

### 10. **Notable Patterns**

1. **CQRS Pattern**: Separate query and mutation resolvers
2. **Repository Pattern**: All DB access through repository with dependency injection
3. **Union-based Error Handling**: All mutations return Success | Error unions
4. **Multi-tenancy**: Context-based tenant filtering
5. **SQL Function Integration**: Direct mapping of SQL functions to GraphQL mutations
6. **View-based Queries**: All queries use PostgreSQL views with JSON projections

This implementation demonstrates a sophisticated use of Strawberry GraphQL, leveraging many of its advanced features while maintaining clean separation of concerns and type safety throughout the application.

## Key Strawberry Functionalities for FraiseQL Replacement

### Core Decorators
- `@strawberry.type` - Define GraphQL object types
- `@strawberry.input` - Define GraphQL input types
- `@strawberry.field` - Define GraphQL fields with custom behavior
- `@strawberry.mutation` - Define mutation fields
- `@strawberry.interface` - Define GraphQL interfaces
- `@strawberry.enum` - Define GraphQL enums
- `@strawberry.scalar` - Define custom scalar types

### Type System Features
1. **Union Types** - Critical for Success | Error pattern
2. **Generic Types** - For pagination responses
3. **Interface Implementation** - Type inheritance
4. **Optional Fields** - With defaults (`field: type | None = default`)
5. **Custom Scalars** - Date, IP, JSON types

### Field Resolution
1. **Async Resolvers** - All queries are async
2. **Dependency Injection** - Via `Annotated[Type, Depends(...)]`
3. **Context Access** - `info.context` for tenant_id, user_id
4. **Field Arguments** - Resolver method parameters

### Schema Features
1. **Schema Assembly** - Combining query/mutation types
2. **Scalar Overrides** - Custom scalar mapping
3. **merge_types** utility - Combining multiple type definitions

### Critical Patterns
1. **Union-based Error Handling** - Every mutation returns Success | Error
2. **Pagination Pattern** - OrderBy, Limit/Offset, PaginatedResponse
3. **Where/Filter Types** - Dynamic filter generation
4. **JSON Scalar** - For flexible metadata fields
5. **Field Customization** - Via `strawberry.field()`

### Integration Points
1. **FastAPI Router Integration**
2. **Context Getter** - For auth/tenant info
3. **GraphQL IDE Support** - GraphiQL

The most critical features to implement are:
- Type decorators with full type inference
- Union types for error handling
- Async resolver support with dependency injection
- Custom scalar support
- Generic types for reusable patterns
