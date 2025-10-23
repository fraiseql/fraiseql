# Core Domain Tables

This directory contains the primary business entity tables that represent the core domain model.

## Structure

### 010_customers/
Customer management tables:
- `0101_tb_customer.sql` - Main customers table
- `0102_tb_address.sql` - Customer addresses

### 020_products/
Product catalog tables:
- `0201_tb_product.sql` - Main products table
- `0202_tb_category.sql` - Product categories
- `0203_tb_product_variant.sql` - Product variations (size, color, etc.)

### 030_orders/
Order management tables:
- `0301_tb_order.sql` - Main orders table
- `0302_tb_order_item.sql` - Individual order line items

## CQRS Architecture

These tables represent the **write model** in our CQRS architecture:
- Tables use `tb_*` naming convention
- Contain normalized business data
- Primary keys and foreign key constraints
- Business logic validation at the database level

## Application

Apply these files in numerical order to establish the core domain schema.
