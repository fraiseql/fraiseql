# Proposed Implementation: Allocation Type with JSONB Nested Objects

Based on your response, here's our proposed implementation for the Allocation type. Please validate if this is correct.

## Current Database Structure

Our `tv_allocation` table has this structure:

```sql
CREATE TABLE public.tv_allocation (
    id uuid NOT NULL,
    machine_id uuid,
    machine_item_id uuid,
    organizational_unit_id uuid,
    location_id uuid,
    valid_from date,
    valid_until date,
    is_past boolean,
    is_current boolean,
    is_future boolean,
    is_reserved boolean,
    is_stock boolean,
    data jsonb,  -- Contains nested objects
    -- ... other fields
);
```

The `data` JSONB column contains:
```json
{
  "identifier": "ALLOC-001",
  "start_date": "2024-01-01",
  "end_date": "2024-12-31",
  "notes": "Some notes",
  "notes_contact": "Contact info",
  "is_provisionnal": false,
  "machine": { /* full machine object */ },
  "location": { /* full location object */ },
  "organizational_unit": { /* full org unit object */ },
  "network_configuration": { /* full network config object */ }
}
```

## Proposed Type Implementation

```python
"""Allocation type for GraphQL."""

import uuid
from datetime import date
from typing import Optional

import fraiseql

# Assuming these types are defined elsewhere with @fraiseql.type
from printoptim_backend.entrypoints.api.resolvers.query.dim.geo.gql_geo_query import Location
from printoptim_backend.entrypoints.api.gql_types.dim.org import OrganizationalUnit
from printoptim_backend.entrypoints.api.gql_types.dim.gql_mat import Machine
from printoptim_backend.entrypoints.api.gql_types.dim.network import NetworkConfiguration


@fraiseql.type
class Allocation:
    """Allocation type representing machine item allocations.

    FraiseQL will automatically instantiate nested types from the JSONB data
    in development mode when fields are properly defined.
    """
    # Direct fields from the table columns
    id: uuid.UUID
    machine_id: Optional[uuid.UUID]
    machine_item_id: Optional[uuid.UUID]
    organizational_unit_id: Optional[uuid.UUID]
    location_id: Optional[uuid.UUID]
    valid_from: date
    valid_until: Optional[date]
    is_past: bool
    is_current: bool
    is_future: bool
    is_reserved: bool
    is_stock: bool

    # Fields from the JSONB 'data' column
    # Question: Will FraiseQL automatically extract these from data->field_name?
    identifier: Optional[str]
    start_date: Optional[date]
    end_date: Optional[date]
    notes: Optional[str]
    notes_contact: Optional[str]
    is_provisionnal: bool = False

    # Nested objects from JSONB - FraiseQL will instantiate these types
    machine: Optional[Machine]
    location: Optional[Location]
    organizational_unit: Optional[OrganizationalUnit]
    network_configuration: Optional[NetworkConfiguration]
```

## Questions

1. **Do we need to modify our database view?**
   - Currently, our view returns a `data` JSONB column with all nested fields
   - Should we extract fields to top level like `data->>'identifier' as identifier`?
   - Or can FraiseQL work with the current structure?

2. **Field mapping**
   - Will FraiseQL automatically map `identifier` field to `data->identifier` from the JSONB column?
   - Or do we need to keep the `data: dict` field and handle extraction differently?

3. **Query usage**
   ```python
   # Our current query
   query = """
       SELECT
           id, machine_id, organizational_unit_id, location_id,
           valid_from, valid_until, is_past, is_current, is_future,
           is_reserved, is_stock, data
       FROM app.tv_allocation
       WHERE tenant_id = $1
   """
   ```
   - Is this query structure compatible with the proposed type definition?

4. **Missing fields**
   - We noticed our view has `valid_from/valid_until` but the JSONB has `start_date/end_date`
   - How should we handle this duplication?

Please confirm if this implementation is correct or if we need adjustments.
