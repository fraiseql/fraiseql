# Mutation Pattern in FraiseQL

FraiseQL uses a structured pattern for mutations that ensures consistent error handling and type safety.

## Basic Mutation Structure

Every mutation in FraiseQL must define three type annotations:

1. **input** - The input data type
2. **success** - The success response type
3. **failure** (or **error**) - The error response type

## Example Mutation

```python
from fraiseql import mutation, fraise_input, fraise_type, success, failure

# Define input type
@fraise_input
class CreateUserInput:
    name: str
    email: str
    password: str

# Define success type
@success
@fraise_type
class CreateUserSuccess:
    user_id: int
    message: str = "User created successfully"

# Define failure type
@failure
@fraise_type
class CreateUserFailure:
    code: str
    message: str

# Define the mutation
@mutation
class CreateUser:
    input: CreateUserInput
    success: CreateUserSuccess
    failure: CreateUserFailure  # Can also use 'error' for backwards compatibility

    async def execute(self, db, input_data):
        # Mutation logic here
        try:
            user_id = await db.create_user(
                name=input_data.name,
                email=input_data.email,
                password=hash_password(input_data.password)
            )
            return CreateUserSuccess(user_id=user_id)
        except UserAlreadyExistsError:
            return CreateUserFailure(
                code="USER_EXISTS",
                message="User with this email already exists"
            )
```

## Using 'failure' vs 'error'

FraiseQL supports both attribute names for flexibility:

### Recommended: Using 'failure'
```python
@mutation
class CreatePost:
    input: CreatePostInput
    success: CreatePostSuccess
    failure: CreatePostFailure  # More intuitive name
```

### Legacy: Using 'error'
```python
@mutation
class CreatePost:
    input: CreatePostInput
    success: CreatePostSuccess
    error: CreatePostError  # Still supported for backwards compatibility
```

## GraphQL Schema Output

The mutation will be exposed in the GraphQL schema as:

```graphql
type Mutation {
  createUser(input: CreateUserInput!): CreateUserResult!
}

union CreateUserResult = CreateUserSuccess | CreateUserFailure

input CreateUserInput {
  name: String!
  email: String!
  password: String!
}

type CreateUserSuccess {
  userId: Int!
  message: String!
}

type CreateUserFailure {
  code: String!
  message: String!
}
```

## Migration from Other Patterns

If you're migrating from Strawberry or other GraphQL libraries:

### Before (Function-based):
```python
async def create_user(info, input: CreateUserInput) -> CreateUserSuccess | CreateUserError:
    # Implementation
```

### After (Class-based with FraiseQL):
```python
@mutation
class CreateUser:
    input: CreateUserInput
    success: CreateUserSuccess
    failure: CreateUserFailure

    async def execute(self, db, input_data):
        # Implementation
```

## Best Practices

1. **Use descriptive failure codes**: Include specific error codes in your failure types
2. **Keep success types focused**: Only include relevant data in success responses
3. **Validate in the mutation**: Perform business logic validation in the execute method
4. **Use the @success and @failure decorators**: They help with union type generation
5. **Prefer 'failure' over 'error'**: It's more descriptive and clearer in intent
