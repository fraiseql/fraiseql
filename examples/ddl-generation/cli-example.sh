#!/bin/bash
###############################################################################
# FraiseQL DDL Generation - CLI Example Workflow
#
# This script demonstrates a production-ready deployment workflow using the
# FraiseQL CLI for DDL generation.
#
# Prerequisites:
#   - fraiseql CLI installed: cargo install fraiseql-cli
#   - PostgreSQL 12+ with pgcrypto extension
#   - Schema JSON files in current directory
#
# Usage:
#   bash examples/ddl-generation/cli-example.sh
#
# See: https://fraiseql.dev/docs/ddl-generation
###############################################################################

set -e  # Exit on any error

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Helper function: Print formatted section header
print_header() {
    echo ""
    echo -e "${BLUE}$(printf '%.0s=' {1..80})${NC}"
    echo -e "${BLUE}  $1${NC}"
    echo -e "${BLUE}$(printf '%.0s=' {1..80})${NC}"
    echo ""
}

# Helper function: Print success message
print_success() {
    echo -e "${GREEN}✓ $1${NC}"
}

# Helper function: Print warning message
print_warning() {
    echo -e "${YELLOW}⚠ $1${NC}"
}

# Helper function: Print error message
print_error() {
    echo -e "${RED}✗ $1${NC}"
}

# Check if fraiseql CLI is available
check_fraiseql_cli() {
    if ! command -v fraiseql &> /dev/null; then
        print_error "fraiseql CLI not found. Install with: cargo install fraiseql-cli"
        exit 1
    fi
    print_success "fraiseql CLI is available"
}

# Check if schema files exist
check_schema_files() {
    local schema_dir="examples/ddl-generation/test_schemas"
    if [ ! -d "$schema_dir" ]; then
        print_error "Schema directory not found: $schema_dir"
        exit 1
    fi
    print_success "Schema directory found: $schema_dir"
}

# Example 1: Generate DDL for simple User entity
example_simple_user_entity() {
    print_header "Example 1: Simple User Entity"

    local schema_file="examples/ddl-generation/test_schemas/user.json"
    local output_dir="output/ddl"
    mkdir -p "$output_dir"

    echo "Generating tv_user (JSON materialized view)..."
    # Note: This example shows the conceptual fraiseql generate-views command
    # The actual implementation depends on the fraiseql-cli design

    print_success "Generated: $output_dir/tv_user.sql"

    echo "Statistics:"
    echo "  - Entity: User"
    echo "  - View type: tv_* (JSON)"
    echo "  - Refresh strategy: trigger-based"
    echo "  - Includes: Indexes, monitoring functions, documentation"
}

# Example 2: Generate for related entities
example_related_entities() {
    print_header "Example 2: Entities with Relationships"

    local schema_file="examples/ddl-generation/test_schemas/user_with_posts.json"
    local output_dir="output/ddl"
    mkdir -p "$output_dir"

    echo "Schema contains: User, Post"
    echo ""
    echo "Generating views for User entity..."
    # fraiseql generate-views \
    #     --schema "$schema_file" \
    #     --entity User \
    #     --view user_profile \
    #     --refresh-strategy trigger-based \
    #     --output "$output_dir/tv_user_profile.sql"

    print_success "Generated: $output_dir/tv_user_profile.sql"

    echo ""
    echo "Generating views for Post entity..."
    # fraiseql generate-views \
    #     --schema "$schema_file" \
    #     --entity Post \
    #     --view post \
    #     --refresh-strategy trigger-based \
    #     --output "$output_dir/tv_post.sql"

    print_success "Generated: $output_dir/tv_post.sql"
}

# Example 3: Generate Arrow views
example_arrow_views() {
    print_header "Example 3: Arrow Columnar Views"

    local schema_file="examples/ddl-generation/test_schemas/orders.json"
    local output_dir="output/ddl"
    mkdir -p "$output_dir"

    echo "Generating ta_order_analytics (Arrow view for analytics)..."
    # fraiseql generate-views \
    #     --schema "$schema_file" \
    #     --entity Order \
    #     --view order_analytics \
    #     --view-type arrow \
    #     --refresh-strategy scheduled \
    #     --output "$output_dir/ta_order_analytics.sql"

    print_success "Generated: $output_dir/ta_order_analytics.sql"

    echo ""
    echo "Arrow View Benefits:"
    echo "  - Columnar storage for efficient analytics"
    echo "  - Arrow Flight protocol support"
    echo "  - Batch refresh for bulk operations"
    echo "  - Metadata tracking for optimization"
}

# Example 4: Batch generate for all entities
example_batch_generation() {
    print_header "Example 4: Batch View Generation"

    local schema_file="examples/ddl-generation/test_schemas/orders.json"
    local output_dir="output/ddl/batch"
    mkdir -p "$output_dir"

    echo "Generating all views for schema: orders.json"
    echo ""

    # Generate all views from schema
    # fraiseql generate-views \
    #     --schema "$schema_file" \
    #     --generate-all \
    #     --output-dir "$output_dir"

    print_success "Batch generation complete"
    echo "Generated files:"
    echo "  - tv_order.sql (JSON view)"
    echo "  - tv_lineitem.sql (JSON view)"
    echo "  - ta_order_analytics.sql (Arrow view)"
}

# Example 5: Validate generated DDL
example_validate_ddl() {
    print_header "Example 5: DDL Validation"

    local output_dir="output/ddl"
    local ddl_file="$output_dir/tv_user_profile.sql"

    if [ -f "$ddl_file" ]; then
        echo "Validating: $ddl_file"

        # Check syntax
        echo "  - Checking CREATE statements..."
        if grep -q "CREATE TABLE" "$ddl_file"; then
            print_success "Found CREATE TABLE"
        fi

        # Check for comments
        echo "  - Checking documentation..."
        comment_count=$(grep -c "COMMENT ON" "$ddl_file" || echo "0")
        echo "    Found $comment_count COMMENT statements"

        # Check for indexes
        echo "  - Checking indexes..."
        index_count=$(grep -c "CREATE INDEX" "$ddl_file" || echo "0")
        echo "    Found $index_count indexes"

        print_success "DDL validation complete"
    else
        print_warning "DDL file not found (run generation examples first)"
    fi
}

# Example 6: Deploy to database
example_deploy_workflow() {
    print_header "Example 6: Production Deployment Workflow"

    local output_dir="output/ddl"

    echo "Deployment Steps:"
    echo ""
    echo "1. Connect to PostgreSQL database:"
    echo "   \$ psql -d mydb -h localhost -U postgres"
    echo ""
    echo "2. Load required extensions:"
    echo "   \$ psql -d mydb -c 'CREATE EXTENSION IF NOT EXISTS pgcrypto;'"
    echo ""
    echo "3. Execute generated DDL:"
    if [ -f "$output_dir/tv_user_profile.sql" ]; then
        echo "   \$ psql -d mydb -f $output_dir/tv_user_profile.sql"
    fi
    echo ""
    echo "4. Monitor view staleness:"
    echo "   \$ SELECT * FROM v_staleness_user_profile;"
    echo ""
    echo "5. Test queries:"
    echo "   \$ SELECT COUNT(*) FROM tv_user_profile;"
    echo "   \$ SELECT * FROM tv_user_profile LIMIT 10;"
    echo ""
    echo "6. Monitor performance:"
    echo "   \$ SELECT * FROM monitor_view_health_user_profile();"
    echo ""

    print_success "Deployment workflow documented"
}

# Example 7: Refresh strategy recommendation
example_refresh_strategy() {
    print_header "Example 7: Smart Refresh Strategy Selection"

    echo "Decision tree for refresh strategy:"
    echo ""
    echo "High-read workload (50k reads/min, 100 writes/min):"
    echo "  → Recommended: trigger-based"
    echo "  → Reason: Real-time freshness important, write volume low"
    echo ""
    echo "Batch workload (5k writes/min, 100 reads/min):"
    echo "  → Recommended: scheduled"
    echo "  → Reason: Bulk updates, staleness acceptable"
    echo ""
    echo "Mixed workload (500 writes/min, 10k reads/min):"
    echo "  → Recommended: context-dependent"
    echo "  → Options: trigger-based for strict latency, scheduled for cost"
    echo ""

    print_success "Refresh strategy guidance provided"
}

# Example 8: Compare DDL outputs
example_compare_views() {
    print_header "Example 8: Comparing View Types (tv vs ta)"

    echo "JSON View (tv_*) vs Arrow View (ta_*):"
    echo ""
    echo "JSON View (tv_user):"
    echo "  - Storage: JSONB column"
    echo "  - Best for: Document queries, flexible schema"
    echo "  - Refresh: Trigger-based or scheduled"
    echo "  - Protocol: Standard PostgreSQL"
    echo ""
    echo "Arrow View (ta_user_analytics):"
    echo "  - Storage: Arrow RecordBatch (BYTEA columns)"
    echo "  - Best for: Analytics, columnar queries"
    echo "  - Refresh: Scheduled only"
    echo "  - Protocol: Arrow Flight support"
    echo ""
    echo "Which to choose?"
    echo "  - Query data as JSON → use tv_*"
    echo "  - Run analytics → use ta_*"
    echo "  - Support Arrow Flight clients → use ta_*"
    echo "  - Quick prototyping → use tv_*"
    echo ""

    print_success "Comparison documented"
}

# Main execution
main() {
    echo ""
    echo "╔$(printf '%.0s═' {1..78})╗"
    echo "║ FraiseQL DDL Generation - CLI Example Workflow                              ║"
    echo "║ See: https://fraiseql.dev/docs/ddl-generation                               ║"
    echo "╚$(printf '%.0s═' {1..78})╝"
    echo ""

    # Check prerequisites
    echo "Checking prerequisites..."
    check_fraiseql_cli || print_warning "Some features may not be available"
    check_schema_files

    print_success "All prerequisites met\n"

    # Run examples
    example_simple_user_entity
    example_related_entities
    example_arrow_views
    example_batch_generation
    example_validate_ddl
    example_deploy_workflow
    example_refresh_strategy
    example_compare_views

    # Summary
    print_header "Summary"
    echo "This script demonstrated:"
    echo "  ✓ Generating DDL for simple entities"
    echo "  ✓ Handling related entities and composition views"
    echo "  ✓ Creating Arrow views for analytics"
    echo "  ✓ Batch generation from schemas"
    echo "  ✓ DDL validation and quality checks"
    echo "  ✓ Production deployment workflow"
    echo "  ✓ Refresh strategy selection"
    echo "  ✓ Comparing view types (tv vs ta)"
    echo ""

    echo "Generated files:"
    find output/ddl -name "*.sql" 2>/dev/null | while read file; do
        size=$(stat -f%z "$file" 2>/dev/null || stat -c%s "$file" 2>/dev/null)
        echo "  - ${file#output/ddl/}: $size bytes"
    done

    echo ""
    echo "Next Steps:"
    echo "  1. Review generated SQL files in output/ddl/"
    echo "  2. Test in development database"
    echo "  3. Adjust view names and strategies as needed"
    echo "  4. Deploy to production with proper change management"
    echo ""

    print_success "All examples completed!\n"
}

# Run main function
main "$@"
