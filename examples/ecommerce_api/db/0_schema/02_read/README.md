# Read Models (CQRS Query Side)

This directory contains views and projection tables for optimized read operations.

## Structure

### 020_customer/
Customer read models:
- `02001_v_customer.sql` - JSONB view of customer data
- `02002_tv_customer.sql` - Projection table for customer queries
- `02003_customer_orders.sql` - Denormalized customer + orders view

### 021_product/
Product read models:
- `02101_v_product.sql` - JSONB view of product data
- `02102_v_category.sql` - JSONB view of category data
- `02103_tv_product.sql` - Projection table for product queries
- `02104_tv_category.sql` - Projection table for category queries
- `02105_products_with_categories.sql` - Joined product + category view

### 022_order/
Order read models:
- `02201_v_order.sql` - JSONB view of order data
- `02202_tv_order.sql` - Projection table for order queries

## CQRS Architecture

These represent the **read model** in our CQRS architecture:
- `v_*` views compute JSONB representations of data
- `tv_*` tables are projection tables populated from views
- Optimized for fast queries with pre-computed relationships
- Updated via explicit sync functions after mutations

## Synchronization

Projection tables are kept in sync through explicit sync functions in the functions directory, ensuring consistency without triggers.
