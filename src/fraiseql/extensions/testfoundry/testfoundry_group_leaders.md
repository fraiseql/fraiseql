# 📜 **README: Group Leader Concept in `testfoundry` Input Generation**

---

# 📚 What is a Group Leader?

In `testfoundry`, a **group leader** is a **special field** that controls the generation of **multiple related fields at once** based on a **foreign key mapping**.

Instead of generating related fields separately (risking inconsistency),
the **group leader** uses **one SQL mapping** to select and populate **all dependent fields together**.

✅ Guarantees **data consistency**
✅ Reduces **random query overhead**
✅ Simplifies **dependency tracking**

---

# 🛠 Why Group Leaders Exist

- Some fields **cannot** be generated independently without breaking relational integrity.
- Some fields **must** be **internally consistent** — e.g., a `postal_code` must match a `country`.
- A **group leader** ensures that **all dependent fields** are populated atomically from the same data source.

---

# ✨ How It Works

When a field is marked as a **group leader**:
- It executes a **single** random FK query.
- The query returns a **JSONB object** containing multiple fields.
- All fields from the JSON are inserted into the generated input at once.
- All dependent fields are marked as **already generated** — no need to generate separately.

---

# 📋 Key Columns in `testfoundry_tb_input_field_mapping`

| Column | Purpose |
|:-------|:--------|
| `generator_group` | Logical group name (e.g., `postal_city_country`) |
| `group_leader` | `TRUE` if this field is responsible for generating the group |
| `group_dependency_fields` | List of fields populated alongside the leader |

---

# ✍ Basic Example: Postal Code Group

## 🧠 Problem:

You want to generate:
- `country`
- `postal_code`
- `city_code`

✅ These must **match each other** — not random separate data!

---

## 📋 Table Definitions

| Field | Group | Group Leader | Dependency Fields |
|:------|:------|:-------------|:------------------|
| `country` | postal_city_country | TRUE | `['country', 'postal_code', 'city_code']` |
| `postal_code` | postal_city_country | FALSE | NULL |
| `city_code` | postal_city_country | FALSE | NULL |

---

## 🛠 Process:

- **country** (group leader) is generated first.
- It uses a `testfoundry_tb_fk_mapping` that returns:
  - `country`
  - `postal_code`
  - `city_code`
- All three fields are inserted into the generated input in one step.

✅ **Consistency guaranteed** — no mismatched postal codes or wrong cities!

---

## Example JSONB returned by group leader:

```json
{
    "country": "France",
    "postal_code": "75001",
    "city_code": "PAR"
}
```

---

# ✍ Complex Example: Product-Driven Hierarchy

## 🧠 Business Problem:

You want to generate a realistic complex business structure for sales transactions:
- **Products** related to **manufacturers**, **models**, **accessories**
- **Contracts** belonging to **organizations** with **financing conditions**
- **Items** built from products + contracts
- **Prices** attached to items
- **Machines** associated with specific prices

✅ Deep dependency chains
✅ Relational consistency mandatory

---

## 📋 High-Level Dependency Flow:

```plaintext
manufacturer
  └── model / accessory
      └── product

organization
  └── contract
      └── financing_condition

product + contract_with_financing_condition
  └── item
      └── price

price ⇄ machine (association)
```

---

## 🛠 Group Leaders Setup:

| Group | Leader Field | Fields Generated |
|:------|:-------------|:-----------------|
| `product_group` | manufacturer | manufacturer, model, accessory, product |
| `organization_contract_group` | organization | organization, contract, financing_condition |
| `item_group` | item | item, price |
| `machine_price_group` | machine | machine, price (association) |

---

### 1. `product_group`

- Leader: `manufacturer`
- Generates: `manufacturer`, `model`, `accessory`, `product`
- Pulls from mapping joining manufacturers, models, accessories, products.

---

### 2. `organization_contract_group`

- Leader: `organization`
- Generates: `organization`, `contract`, `financing_condition`
- Pulls from mapping joining organization and its contract terms.

---

### 3. `item_group`

- Leader: `item`
- Generates: `item`, `price`
- Depends on previously generated `product` and `contract` with `financing_condition`.

---

### 4. `machine_price_group`

- Leader: `machine`
- Generates: `machine`, linked `price` via an association table.

---

## 🧠 Why group leaders are critical here:

Without group leaders, you could easily end up with:

- Products belonging to one manufacturer but model of another.
- Financing conditions invalid for the selected contract.
- Machines referencing the wrong price plans.

Using group leaders:

✅ All linked fields are generated consistently.
✅ No mismatched FKs.
✅ Strong business logic preserved.

---

# 📋 Realistic Example JSONB for Product Group:

```json
{
  "manufacturer": "Caterpillar",
  "model": "D6 Dozer",
  "accessory": "Extended Blade",
  "product": "D6 Dozer with Extended Blade"
}
```

And for Organization Contract Group:

```json
{
  "organization": "ACME Corp",
  "contract": "3-Year Maintenance Agreement",
  "financing_condition": "Low-Interest Financing, 24 months"
}
```

Combined into an Item Group:

```json
{
  "item": "D6 Dozer Lease Plan",
  "price": 149999
}
```

---

# 🎯 Group Leader Best Practices

| Rule | Why |
|:-----|:----|
| Group leaders must generate **all** necessary fields together | ✅ Guarantees relational consistency |
| Only **one** group leader per group | ✅ Prevents conflicts |
| List **dependent fields** explicitly in `group_dependency_fields` | ✅ So generator can know which fields are covered |
| FK mapping for leader must **select all required columns** | ✅ Otherwise missing fields |
| Skip already generated fields in retry passes | ✅ No duplication |

---

# 🏆 Conclusion

The **Group Leader** concept in `testfoundry` is a **critical design** for:
- Building complex, relationally-valid, random input data.
- Generating **GraphQL mutations** automatically for integration testing.
- Keeping large business entity graphs **consistent** and **meaningful**.

Mastering Group Leaders = mastering real-world test data generation. 🚀
