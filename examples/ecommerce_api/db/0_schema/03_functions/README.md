# Business Logic Functions

This directory contains the application and core layer functions that implement business logic.

## Architecture

Functions follow a clean architecture pattern with two layers:

### App Layer (`app.*` functions)
- **Purpose**: API-facing functions called by application code
- **Responsibilities**:
  - Input validation and transformation
  - Orchestration of business operations
  - Return ultra-direct JSONB responses for Rust transformer
- **Security**: `SECURITY DEFINER` to run with elevated privileges

### Core Layer (`core.*` functions)
- **Purpose**: Pure business logic implementation
- **Responsibilities**:
  - Business rule validation
  - Data manipulation
  - Transaction management
  - Explicit projection table synchronization

## Structure

### 010_customer_functions/ (Legacy)
Older customer functions without proper app/core separation.

### 030_customer_functions/
Modern customer CRUD operations:
- `0301_create_customer.sql` - Customer creation
- `0302_update_customer.sql` - Customer updates
- `0303_delete_customer.sql` - Customer deletion
- `0304_sync_customer.sql` - Projection sync functions

### 031_product_functions/
Product CRUD operations:
- `0311_create_product.sql` - Product creation
- `0312_update_product.sql` - Product updates
- `0313_delete_product.sql` - Product deletion
- `0314_sync_product.sql` - Projection sync functions

### 032_order_functions/
Order CRUD operations:
- `0321_create_order.sql` - Order creation with items
- `0322_update_order.sql` - Order status updates
- `0323_delete_order.sql` - Order deletion (pending orders only)
- `0324_sync_order.sql` - Projection sync functions

## Ultra-Direct Mutation Response

All app functions return simple JSONB responses optimized for the Rust transformer:

```json
{
  "success": true,
  "code": "SUCCESS",
  "message": "Operation completed",
  "data": { "entity": { "id": "...", ... } }
}
```

This provides 10-80x performance improvement over complex Debezium-style events.
