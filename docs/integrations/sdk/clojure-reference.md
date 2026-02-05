# FraiseQL Clojure SDK Reference

**Status**: Production-Ready | **Clojure Version**: 1.11+ | **SDK Version**: 2.0.0+
**Last Updated**: 2026-02-05 | **Maintained By**: FraiseQL Community

Complete API reference for the FraiseQL Clojure SDK. This guide covers the complete Clojure authoring interface for building type-safe GraphQL APIs with macros, persistent data structures, and functional composition patterns. Emphasizes Clojure's LISP heritage, homoiconicity, and data-as-code philosophy.

## Installation & Setup

### Leiningen / Deps.edn

**Leiningen** (`project.clj`):

```clojure
(defproject my-FraiseQL-api "1.0.0"
  :dependencies [[org.clojure/clojure "1.11.1"]
                 [FraiseQL/SDK "2.0.0"]
                 [org.clojure/spec.alpha "0.5.228"]])
```

**Deps.edn**:

```clojure
{:deps {org.clojure/clojure {:mvn/version "1.11.1"}
        FraiseQL/SDK {:mvn/version "2.0.0"}
        org.clojure/spec.alpha {:mvn/version "0.5.228"}}}
```

**Requirements**: Clojure 1.11+, JDK 11+, Leiningen 2.9+ or Clojure CLI

### First Schema (30 seconds)

```clojure
(ns my-app.schema
  (:require [FraiseQL.core :as fql]))

(fql/defschema User
  :id :int
  :name :string
  :email {:type :string :nullable true})

(fql/defquery users
  :sql-source "v_users"
  :params {:limit {:type :int :default 10}}
  :returns [User])

(fql/export-schema! "schema.json")
```

Compile and deploy:

```bash
FraiseQL-cli compile schema.json FraiseQL.toml
FraiseQL-server --schema schema.compiled.json
```

---

## Quick Reference Table

| Feature | Macro | Purpose | Homoiconicity |
|---------|-------|---------|---|
| **Type Definition** | `defschema` | GraphQL object types | Code is data |
| **Query Operation** | `defquery` | Read operations (SELECT) | Composition-ready |
| **Mutation Operation** | `defmutation` | Write operations (INSERT/UPDATE/DELETE) | Data-driven |
| **Fact Table** | `deffacttable` | Analytics tables (OLAP) | Transform-safe |
| **Aggregate Query** | `defagg-query` | Analytics queries | Reducible |
| **Validator** | `defvalidator` | Field validation | Composable |
| **Security RBAC** | `defsecurity` | Role-based access control | Policy-as-data |
| **Subscription** | `defsubscription` | Real-time pub/sub | Async-ready |

---

## Type System

### The `defschema` Macro

Define GraphQL object types using persistent maps and keyword-keyed arguments:

```clojure
(fql/defschema TypeName
  :field1 :int :field2 :string :field3 :boolean)
```

**Key Features**: Keywords for clarity, nullability via maps, nested types, lists, docstrings, immutable by default.

**Examples**:

```clojure
; Simple schema
(fql/defschema User
  "A user account."
  :id :int :username :string :email :string)

; Optional and nested fields
(fql/defschema BlogPost
  :id :int :title :string :author {:type User}
  :tags {:type :string :list true}
  :created-at :datetime
  :updated-at {:type :datetime :nullable true})

; Reuse via functional composition
(def timestamped
  {:created-at :datetime :updated-at :datetime
   :deleted-at {:type :datetime :nullable true}})

(fql/defschema AuditedEntity
  :id :int
  (merge timestamped))

; With spec.alpha validation
(require '[clojure.spec.alpha :as s])
(s/def ::positive-int (s/and int? pos?))

(fql/defschema Product
  :id :int :name :string
  :price {:type :decimal :spec ::positive-int})
```

### Nested Schemas and Composition

Leverage Clojure's data manipulation for composable schemas:

```clojure
(fql/defschema Address
  :street :string :city :string :state :string)

(fql/defschema Person
  :id :int :name :string
  :address {:type Address})

; Compose schemas functionally
(defn with-audit [schema-def]
  (merge schema-def
         {:created-by :string :created-at :datetime}))

(fql/defschema AuditedProduct
  :id :int :name :string
  (with-audit {}))
```

---

## Operations

### Queries: Read Operations

Read-only operations using `defquery` macro for declarative, composable definitions:

```clojure
(fql/defquery query-name
  "Documentation."
  :sql-source "view_name"
  :params {:arg1 :int}
  :returns [ResultType])
```

**Examples**:

```clojure
; Simple list query
(fql/defquery users
  "Get all users with pagination."
  :sql-source "v_users"
  :params {:limit {:type :int :default 10}}
  :returns [User])

; Single result query
(fql/defquery user-by-id
  "Get a user by ID."
  :sql-source "v_user_by_id"
  :params {:id :int}
  :returns User)

; Multi-parameter search
(fql/defquery search-users
  "Search by name and email."
  :sql-source "v_search_users"
  :params {:name :string
           :email {:type :string :nullable true}
           :limit {:type :int :default 20}
           :offset {:type :int :default 0}}
  :returns [User])

; Cached query
(fql/defquery trending-items
  "Cached for 5 minutes."
  :sql-source "v_trending"
  :cache-ttl 300
  :params {:limit {:type :int :default 10}}
  :returns [Item])

; Higher-order function for DRY queries
(defn paginated-query [query-name sql-source result-type]
  {:query-name query-name :sql-source sql-source
   :params {:limit {:type :int :default 20}
            :offset {:type :int :default 0}}
   :returns result-type})

(fql/defquery products
  "Paginated product list."
  (paginated-query :products "v_products" [Product]))
```

### Mutations: Write Operations

Write operations using `defmutation` macro for explicit operation typing:

```clojure
(fql/defmutation mutation-name
  "Documentation."
  :sql-source "fn_name"
  :operation :create  ; :create | :update | :delete | :custom
  :params {:arg1 :string}
  :returns ResultType)
```

**Examples**:

```clojure
; Create mutation
(fql/defmutation create-user
  "Create a new user."
  :sql-source "fn_create_user"
  :operation :create
  :params {:name :string :email :string}
  :returns User)

; Update mutation
(fql/defmutation update-user
  "Update existing user."
  :sql-source "fn_update_user"
  :operation :update
  :params {:id :int
           :name {:type :string :nullable true}}
  :returns User)

; Delete mutation
(fql/defmutation delete-user
  "Delete user account."
  :sql-source "fn_delete_user"
  :operation :delete
  :params {:id :int}
  :returns :int)

; Transactional custom mutation
(fql/defmutation transfer-funds
  "Atomic fund transfer."
  :sql-source "fn_transfer"
  :operation :custom
  :transaction-isolation :serializable
  :params {:from-id :int :to-id :int :amount :decimal}
  :returns {:status :string :from-balance :decimal :to-balance :decimal})
```

---

## Advanced Features

### Fact Tables for Analytics

OLAP-style analytics with dimensional aggregation:

```clojure
(fql/deffacttable sales-fact
  "Daily sales fact table."
  :measures {:total-revenue :decimal :item-count :int}
  :dimensions {:date-key :int :product-key :int}
  :sql-source "fact_sales")

(fql/defagg-query revenue-by-product
  "Aggregate sales by product."
  :source sales-fact
  :group-by [:product-key]
  :metrics {:total-revenue :sum}
  :returns [{:product-key :int :total-revenue :decimal}])
```

### Role-Based Access Control

RBAC as persistent data structures:

```clojure
(fql/defsecurity admin-access
  :required-roles [:admin]
  :field-mask {:*-all true})

(fql/defsecurity user-access
  :required-roles [:authenticated]
  :field-mask {:id true :name true}
  :row-filter (fn [ctx]
                {:user-id (get-in ctx [:user :id])}))

(fql/defquery current-user
  :sql-source "v_current_user"
  :security user-access
  :returns User)

; Composable security policies
(defn team-access [team-id]
  {:required-roles [:authenticated]
   :field-mask {:id true :name true}
   :row-filter (fn [ctx] {:team-id team-id})})

(fql/defquery team-members
  :sql-source "v_team_members"
  :security (team-access 123)
  :params {:team-id :int}
  :returns [User])
```

### Field Metadata and Directives

Metadata is first-class in Clojure:

```clojure
; Using with-meta
(fql/defschema Product
  :id :int
  :name (with-meta :string
          {:description "Product name"})
  :price (with-meta :decimal
          {:description "Price in USD"}))

; Map-based metadata
(fql/defschema Event
  :id :int
  :name {:type :string :description "Title" :index true}
  :timestamp {:type :datetime :immutable true})

; Custom directives
(def audit-directive
  {:name :audit :applies-to [:field-definition]})

(defn with-audit [field-def]
  (assoc field-def :directives [audit-directive]))

(fql/defschema AuditedUser
  :ssn (with-audit {:type :string}))
```

---

## Scalar Types

Clojure embraces Lisp's symbol system for type representation:

```clojure
(fql/defschema Event
  :id :int :timestamp :datetime :date :date
  :uuid :uuid :json :json :bytes :bytes
  :big-int :bigint :decimal :decimal :float :float
  :boolean :boolean :string :string)
```

| Clojure Keyword | GraphQL Type | Clojure Equivalent |
|---|---|---|
| `:int` | Int | integer |
| `:string` | String | String |
| `:boolean` | Boolean | boolean |
| `:datetime` | DateTime | OffsetDateTime |
| `:date` | Date | LocalDate |
| `:decimal` | Decimal | BigDecimal |
| `:uuid` | UUID | java.util.UUID |
| `:json` | JSON | PersistentMap |
| `:bigint` | BigInt | BigInteger |
| `:float` | Float | double |

---

## Schema Export

Export schemas to JSON for compilation:

```clojure
(ns my-app.core
  (:require [FraiseQL.core :as fql]))

; Single-file export
(fql/export-schema! "schema.json")

; Export with metadata
(fql/export-schema! "schema.json"
  {:version "1.0.0"
   :description "My FraiseQL API"})

; Programmatic inspection (schema-as-data)
(def my-schema
  (fql/build-schema
    {:types [User Product]
     :queries [all-users user-by-id]
     :mutations [create-user]}))

; Custom export logic
(defn export-with-validation [schema filename]
  (when (fql/validate-schema schema)
    (spit filename (fql/->json schema))))
```

---

## Type Mapping

Clojure types map directly to GraphQL and SQL:

```clojure
; Clojure → GraphQL → SQL (PostgreSQL)
:int           → Int!           → integer
:string        → String!        → text
:boolean       → Boolean!       → boolean
:datetime      → DateTime!      → timestamp with time zone
:decimal       → Decimal!       → numeric
:json          → JSON!          → jsonb

; Nullable types
{:type :int :nullable true}     → Int
{:type :string :nullable true}  → String

; Lists
{:type :int :list true}        → [Int!]!
{:type :string :list true}     → [String!]!
```

---

## Common Patterns

### CRUD Operations with Data Composition

Use higher-order functions for DRY CRUD:

```clojure
(defn crud-base [entity-name]
  {:entity-name (keyword entity-name) :params-base {:id :int}})

(defn with-list-query [base result-type]
  (assoc base :list-query
    {:sql-source (str "v_" (:entity-name base) "s")
     :params {:limit {:type :int :default 20}
              :offset {:type :int :default 0}}
     :returns [result-type]}))

(let [user-crud (-> (crud-base :user)
                    (with-list-query User))]
  (fql/defquery list-users (:list-query user-crud)))
```

### Pagination and Transducers

Leverage Clojure's transducers for efficient pagination:

```clojure
(defn paginate-params [page-size]
  {:limit {:type :int :default page-size}
   :offset {:type :int :default 0}})

(fql/defquery search-results
  :sql-source "v_search"
  :params {:query :string
           (paginate-params 50)}
  :returns [SearchResult])
```

### Spec Validation

Use `clojure.spec.alpha` for declarative validation:

```clojure
(require '[clojure.spec.alpha :as s])

(s/def ::positive-int (s/and int? pos?))
(s/def ::email (s/and string? (partial re-matches #".+@.+")))

(fql/defschema User
  :id :int
  :email {:type :string :spec ::email}
  :age {:type :int :spec ::positive-int})

(fql/defvalidator validate-user
  :spec (s/multi-spec ::user :type))
```

---

## Error Handling

Functional error propagation with exception handling:

```clojure
; Try-catch with error context
(try
  (fql/execute! :user-by-id {:id 123})
  (catch ExceptionInfo e
    (case (:type (ex-data e))
      :parse-error (println "Parse error")
      :validation-error (println "Validation failed")
      (throw e))))

; Safe query pattern
(defn safe-query [query-name args]
  (try
    {:ok true :result (fql/execute! query-name args)}
    (catch Exception e
      {:ok false :error (ex-message e)})))

; Input validation with spec
(s/def ::user-id pos-int?)

(defn validated-query [query-name args]
  (if (s/valid? (keyword (str query-name "-args")) args)
    (fql/execute! query-name args)
    (throw (ex-info "Invalid arguments"
                    {:type :validation-error}))))
```

---

## Testing

Use `clojure.test` with immutable data structures:

```clojure
(ns my-app.test.schema
  (:require [clojure.test :refer :all]
            [FraiseQL.core :as fql]))

; Test schema definition
(deftest test-user-schema
  (is (= :User (fql/schema-name User)))
  (is (contains? (fql/schema-fields User) :id)))

; Test query execution
(deftest test-users-query
  (let [result (fql/execute! :users {:limit 5})]
    (is (vector? result))
    (is (every? #(contains? % :id) result))))

; Test mutations
(deftest test-create-user
  (let [user {:name "Alice" :email "alice@example.com"}
        result (fql/execute! :create-user user)]
    (is (contains? result :id))
    (is (= "Alice" (:name result)))))

; Test error cases
(deftest test-invalid-params
  (is (thrown? ExceptionInfo
        (fql/execute! :user-by-id {:id "invalid"}))))

; Fixture with immutable setup
(use-fixtures :each
  (fn [test-fn]
    (let [test-db {:users [{:id 1 :name "Alice"}]}]
      (with-redefs [fql/database test-db]
        (test-fn)))))
```

---

## See Also

- **[FraiseQL Python SDK Reference](./python-reference.md)** - Python authoring interface
- **[FraiseQL TypeScript SDK Reference](./typescript-reference.md)** - TypeScript authoring interface
- **[FraiseQL Go SDK Reference](./go-reference.md)** - Go authoring interface
- **[FraiseQL Java SDK Reference](./java-reference.md)** - Java authoring interface
- **[RBAC Documentation](../../enterprise/rbac.md)** - Role-based access control
- **[Audit Logging](../../enterprise/audit-logging.md)** - Compliance and auditing
- **[Architecture Principles](../../ARCHITECTURE_PRINCIPLES.md)** - System design
- **Clojure Resources**:
  - [Official Clojure Guide](https://clojure.org/guides/getting_started)
  - [Clojure Spec Guide](https://clojure.org/guides/spec)
  - [Leiningen Documentation](https://leiningen.org/)
  - [Core.async for async operations](https://github.com/clojure/core.async)

---

## Troubleshooting

### Common Setup Issues

#### Leiningen Dependency Issues

**Issue**: `Could not find artifact FraiseQL:FraiseQL-clojure`

**Solution**:

```clojure
; project.clj
(defproject myapp "0.1.0"
  :dependencies [[org.clojure/clojure "1.11.0"]
                 [FraiseQL/FraiseQL-clojure "2.0.0"]])
```

```bash
lein deps
```

#### Java Version Issues

**Issue**: `Unsupported Java version`

**Check version** (11+ required):

```bash
java -version
```

**Set in project.clj**:

```clojure
:java-source-paths ["src/java"]
:source-paths ["src/clj"]
:target-path "target/%s"
```

#### Macro Compilation Issues

**Issue**: `No such var: FraiseQL/type`

**Solution - Require properly**:

```clojure
(ns myapp.schema
  (:require [FraiseQL.core :as fq]))

(fq/deftype User
  {:id :int
   :email :string})
```

#### REPL Issues

**Issue**: `CompilerException`

**Solution - Refresh in REPL**:

```clojure
(require :reload 'FraiseQL.core)
```

---

### Type System Issues

#### Map Spec Issues

**Issue**: `ExceptionInfo: Invalid schema structure`

**Solution - Proper schema**:

```clojure
; ✅ Correct
(fq/deftype User
  {:id int?
   :email string?
   :created-at inst?})

; ✅ With optional
(fq/deftype User
  {:id int?
   :email string?
   :bio (nilable? string?)})
```

#### Spec Validation Issues

**Issue**: `ExceptionInfo: failed: ...`

**Solution - Use s/explain**:

```clojure
(require '[clojure.spec.alpha :as s])

(s/explain ::user-spec user-data)
(s/valid? ::user-spec user-data)
```

#### Transducer Issues

**Issue**: `ClassCastException: Transducer expected`

**Solution - Use proper transducers**:

```clojure
; ✅ Correct
(transduce
  (comp (map process-row)
        (filter valid?))
  conj
  results)
```

---

### Runtime Errors

#### Lazy Sequence Issues

**Issue**: `OutOfMemoryError` with large sequences

**Solution - Force realization carefully**:

```clojure
; ✅ With doall
(doall (map FraiseQL/execute queries))

; ✅ Or use reduce
(reduce FraiseQL/execute-accumulated [] queries)
```

#### Thread Pool Issues

**Issue**: `RejectedExecutionException`

**Solution - Limit concurrency**:

```clojure
(require '[clojure.core.async :as async])

(let [chan (async/chan 10)]
  ; Max 10 concurrent tasks
  (async/go-loop []
    (when-let [query (async/<! chan)]
      (FraiseQL/execute query)
      (recur))))
```

#### Map/Vector Issues

**Issue**: `No such namespace: FraiseQL`

**Solution - Require namespace**:

```clojure
(ns myapp.core
  (:require [FraiseQL.core :as fq]))

(fq/execute query variables)
```

---

### Performance Issues

#### Compilation Slowdown

**Issue**: Compile takes >30 seconds

**Use AOT selectively**:

```clojure
; project.clj
:aot [myapp.core]  ; Only what needed
```

#### Lazy Sequence Memory Issues

**Issue**: Memory grows with lazy sequences

**Realize in chunks**:

```clojure
; ✅ Process in batches
(->> (fq/query-stream query)
     (partition 100)
     (map process-batch))
```

#### Database Pool Issues

**Issue**: `SQLException: Cannot get a connection`

**Configure pool**:

```clojure
(require '[hikari-cp.core :as hikari])

(def datasource
  (hikari/make-datasource
    {:jdbc-url "postgresql://..."
     :maximum-pool-size 20
     :minimum-idle-size 5}))
```

---

### Debugging Techniques

#### REPL Debugging

```clojure
user> (require 'FraiseQL.core)
user> (def server (FraiseQL.core/from-compiled "schema.json"))
user> (FraiseQL.core/execute server "{ user(id: 1) { id } }")
```

#### Tap Debugging

```clojure
; Modern Clojure (1.10.0+)
(tap> result)  ; Send to tap

; In terminal
(add-tap println)
```

#### Print Debugging

```clojure
(let [result (FraiseQL/execute query)]
  (prn "Result:" result)
  result)
```

#### Spec Explain

```clojure
(require '[clojure.spec.alpha :as s])

(s/explain ::user-schema user-data)
(clojure.spec.alpha/spec-explain ::user-schema user-data)
```

---

### Getting Help

Provide:

1. Clojure version: `clojure --version`
2. Java version: `java -version`
3. FraiseQL version: `lein deps :tree`
4. Error message
5. Stack trace

**Template**:

```markdown
**Environment**:
- Clojure: 1.11.0
- Java: 11
- FraiseQL: 2.0.0

**Issue**:
[Describe]

**Code**:
[Minimal example]

**Error**:
[Full stack trace]
```

---

**Remember**: Clojure emphasizes data as code (homoiconicity), immutability by default, and functional composition. Leverage persistent data structures, macros for abstraction, and transducers for efficient data transformation. FraiseQL's Clojure SDK makes schema definitions data-driven and composable—schema is code, and code can generate code.
