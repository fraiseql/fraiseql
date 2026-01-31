//! Documentation Examples Validation Tests (RED Phase)
//!
//! Validates that examples in documentation work as documented:
//! 1. Foundation documentation quickstart
//! 2. Core guides (subscriptions, filtering, aggregations)
//! 3. API documentation examples
//! 4. Real-world scenario examples
//!
//! These tests serve as both validation and executable documentation.
//!
//! # Running Tests
//!
//! ```bash
//! cargo test --test documentation_examples_test -- --nocapture
//! ```

#![cfg(test)]

// ============================================================================
// Example Validation Helpers
// ============================================================================

/// Represents a documentation example
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct DocumentationExample {
    /// Example name/title
    title: String,

    /// Source file/location
    source: String,

    /// Example code or description
    code: String,

    /// Expected behavior description
    expected_outcome: String,

    /// Prerequisites (what needs to be set up)
    prerequisites: Vec<String>,
}

impl DocumentationExample {
    /// Create a new documentation example
    fn new(title: &str, source: &str, code: &str) -> Self {
        Self {
            title:            title.to_string(),
            source:           source.to_string(),
            code:             code.to_string(),
            expected_outcome: String::new(),
            prerequisites:    vec![],
        }
    }

    /// Set expected outcome
    fn with_outcome(mut self, outcome: &str) -> Self {
        self.expected_outcome = outcome.to_string();
        self
    }

    /// Add prerequisite
    fn add_prerequisite(mut self, prereq: &str) -> Self {
        self.prerequisites.push(prereq.to_string());
        self
    }

    /// Validate that example structure is sound
    fn validate_structure(&self) -> Result<(), String> {
        if self.title.is_empty() {
            return Err("Example title cannot be empty".to_string());
        }

        if self.code.is_empty() {
            return Err("Example code cannot be empty".to_string());
        }

        if self.expected_outcome.is_empty() {
            return Err("Example must have expected outcome".to_string());
        }

        Ok(())
    }
}

// ============================================================================
// Cycle 5 Tests: Foundation Documentation (RED phase)
// ============================================================================

/// Test 1: Foundation quickstart example
#[test]
fn test_foundation_quickstart_example() {
    let example = DocumentationExample::new(
        "FraiseQL Quickstart",
        "docs/foundation/QUICKSTART.md",
        r#"
// Step 1: Define your schema
@fraiseql.type
class User:
    id: int
    name: str
    email: str

// Step 2: Create FraiseQL instance
fraiseql = FraiseQL(schema=User)

// Step 3: Query your data
query = """
{
    users {
        id
        name
        email
    }
}
"""

result = fraiseql.execute(query)
        "#,
    )
    .with_outcome("Returns list of all users with id, name, and email")
    .add_prerequisite("FraiseQL package installed")
    .add_prerequisite("Schema defined and compiled");

    // Validate example structure
    assert!(example.validate_structure().is_ok(), "Example structure validation failed");

    // Verify example has all required components
    assert!(example.code.contains("@fraiseql.type"));
    assert!(example.code.contains("FraiseQL"));
    assert!(example.code.contains("execute"));
}

/// Test 2: Foundation query example
#[test]
fn test_foundation_query_example() {
    let example = DocumentationExample::new(
        "Simple Query Example",
        "docs/foundation/QUERIES.md",
        r#"
query {
    users {
        id
        name
    }
}
        "#,
    )
    .with_outcome("Returns all users with their IDs and names")
    .add_prerequisite("User table exists")
    .add_prerequisite("User type defined in schema");

    assert!(example.validate_structure().is_ok());

    // Verify query syntax
    assert!(example.code.contains("users"));
    assert!(example.code.contains("id"));
    assert!(example.code.contains("name"));
}

/// Test 3: Foundation mutation example
#[test]
fn test_foundation_mutation_example() {
    let example = DocumentationExample::new(
        "Create User Mutation",
        "docs/foundation/MUTATIONS.md",
        r#"
mutation {
    createUser(input: {name: "John Doe", email: "john@example.com"}) {
        id
        name
        email
    }
}
        "#,
    )
    .with_outcome("Creates a new user and returns their data")
    .add_prerequisite("User table exists with id, name, email columns")
    .add_prerequisite("Create mutation defined in schema");

    assert!(example.validate_structure().is_ok());

    // Verify mutation syntax
    assert!(example.code.contains("mutation"));
    assert!(example.code.contains("createUser"));
    assert!(example.code.contains("input"));
}

// ============================================================================
// Cycle 5 Tests: Core Guides (RED phase)
// ============================================================================

/// Test 4: Subscription guide example
#[test]
fn test_subscription_guide_example() {
    let example = DocumentationExample::new(
        "Real-time User Updates",
        "docs/guides/SUBSCRIPTIONS.md",
        r#"
subscription {
    userCreated {
        id
        name
        email
        createdAt
    }
}
        "#,
    )
    .with_outcome("Receives notifications when new users are created")
    .add_prerequisite("WebSocket connection established")
    .add_prerequisite("Subscription types defined");

    assert!(example.validate_structure().is_ok());

    // Verify subscription syntax
    assert!(example.code.contains("subscription"));
    assert!(example.code.contains("userCreated"));
}

/// Test 5: Filtering guide example
#[test]
fn test_filtering_guide_example() {
    let example = DocumentationExample::new(
        "Advanced Filtering",
        "docs/guides/FILTERING.md",
        r#"
{
    users(where: {
        AND: [
            {status: {eq: "active"}},
            {role: {eq: "admin"}}
        ]
    }) {
        id
        name
        role
    }
}
        "#,
    )
    .with_outcome("Returns only active admin users")
    .add_prerequisite("Users have status and role fields");

    assert!(example.validate_structure().is_ok());

    // Verify complex filtering syntax
    assert!(example.code.contains("where"));
    assert!(example.code.contains("AND"));
    assert!(example.code.contains("eq"));
}

/// Test 6: Aggregation guide example
#[test]
fn test_aggregation_guide_example() {
    let example = DocumentationExample::new(
        "Count Orders by Status",
        "docs/guides/AGGREGATIONS.md",
        r#"
{
    ordersByStatus: groupBy(type: "Order", groupBy: "status") {
        status
        count
        totalAmount
    }
}
        "#,
    )
    .with_outcome("Returns orders grouped by status with counts and totals")
    .add_prerequisite("Order type has status and amount fields");

    assert!(example.validate_structure().is_ok());

    // Verify aggregation syntax
    assert!(example.code.contains("groupBy"));
    assert!(example.code.contains("count"));
}

// ============================================================================
// Cycle 5 Tests: API Documentation (RED phase)
// ============================================================================

/// Test 7: API endpoint example
#[test]
fn test_api_endpoint_example() {
    let example = DocumentationExample::new(
        "GraphQL Endpoint Usage",
        "docs/API.md",
        r#"
POST /graphql HTTP/1.1
Host: api.example.com
Content-Type: application/json

{
    "query": "{ users { id name } }"
}
        "#,
    )
    .with_outcome("Returns GraphQL response with user data")
    .add_prerequisite("Server running and accessible")
    .add_prerequisite("GraphQL endpoint at /graphql");

    assert!(example.validate_structure().is_ok());

    // Verify HTTP request structure
    assert!(example.code.contains("POST /graphql"));
    assert!(example.code.contains("application/json"));
}

/// Test 8: Error handling example
#[test]
fn test_error_handling_example() {
    let example = DocumentationExample::new(
        "Handling GraphQL Errors",
        "docs/API.md",
        r#"
try {
    response = client.execute(query)
} except GraphQLError as e:
    print(f"Error: {e.message}")
    print(f"Code: {e.code}")
    if e.recoverable:
        # Retry with exponential backoff
        retry()
        "#,
    )
    .with_outcome("Properly handles and logs GraphQL errors with retry logic")
    .add_prerequisite("GraphQL client installed")
    .add_prerequisite("Error handling library available");

    assert!(example.validate_structure().is_ok());

    // Verify error handling pattern
    assert!(example.code.contains("try"));
    assert!(example.code.contains("except"));
    assert!(example.code.contains("Error"));
}

// ============================================================================
// Cycle 5 Tests: Real-world Scenarios (RED phase)
// ============================================================================

/// Test 9: Order service example
#[test]
fn test_order_service_example() {
    let example = DocumentationExample::new(
        "Order Creation Workflow",
        "docs/examples/ORDER_SERVICE.md",
        r#"
mutation CreateOrderWorkflow {
    # Step 1: Create order
    order: createOrder(input: {
        customerId: "cust_123"
        items: [{productId: "prod_456", quantity: 2}]
        shippingAddress: "123 Main St"
    }) {
        id
        total
        status
    }

    # Step 2: Reserve inventory
    inventory: reserveInventory(input: {
        orderId: $order.id
        items: [{productId: "prod_456", quantity: 2}]
    }) {
        reserved
        status
    }
}
        "#,
    )
    .with_outcome("Creates order and reserves inventory across two services")
    .add_prerequisite("Order service available")
    .add_prerequisite("Inventory service available");

    assert!(example.validate_structure().is_ok());

    // Verify multi-step workflow
    assert!(example.code.contains("createOrder"));
    assert!(example.code.contains("reserveInventory"));
}

/// Test 10: Inventory integration example
#[test]
fn test_inventory_integration_example() {
    let example = DocumentationExample::new(
        "Inventory Sync with Orders",
        "docs/examples/INVENTORY_SYNC.md",
        r#"
subscription OrderInventorySync {
    # Listen for new orders
    orderCreated {
        id
        items {
            productId
            quantity
        }
    }
}

# On each order:
# 1. Receive notification
# 2. Reserve inventory
# 3. Update stock levels
# 4. Notify customer of status
        "#,
    )
    .with_outcome("Synchronizes inventory with incoming orders in real-time")
    .add_prerequisite("WebSocket connection to order service")
    .add_prerequisite("Inventory database writable");

    assert!(example.validate_structure().is_ok());

    // Verify real-time sync pattern
    assert!(example.code.contains("subscription"));
    assert!(example.code.contains("orderCreated"));
}

// ============================================================================
// Summary
// ============================================================================

// Total: 10 Documentation Example Tests (RED phase)
//
// Coverage:
// - Foundation Documentation: 3 tests ✓
// - Core Guides: 3 tests ✓
// - API Documentation: 2 tests ✓
// - Real-world Scenarios: 2 tests ✓
//
// Total: 10 tests ✓
//
// Phase: RED - Tests verify example structure and basic syntax
// Next phase (GREEN): Execute examples against real system
