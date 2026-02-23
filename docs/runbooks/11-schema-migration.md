# Runbook: Schema Migration (Compiled Schema Update)

## Symptoms

- New FraiseQL server version incompatible with current compiled schema
- Need to update GraphQL type definitions or queries
- Query compatibility issues after schema change
- New database fields not available in GraphQL API
- Schema validation failures: `schema.compiled.json is invalid`
- Version mismatch: `compiled schema version 1.0 incompatible with server version 2.0`
- Missing GraphQL fields that clients expect
- Authorization rules changed (field permissions)

## Impact

- **Standard**: Requires coordinated deployment
- Schema updates may be backward compatible or breaking
- Clients may need updates if breaking schema changes
- Rate limiting/authorization rules may change
- Query results may change (e.g., field type changes)

## Investigation

### 1. Current Schema Status

```bash
# Check current compiled schema version
jq '.metadata.version' /etc/fraiseql/schema.compiled.json

# Check schema modification date
ls -lah /etc/fraiseql/schema.compiled.json

# Verify schema is valid JSON
jq empty /etc/fraiseql/schema.compiled.json && echo "✓ Valid JSON" || echo "✗ Invalid JSON"

# Get schema summary
jq '{
  version: .metadata.version,
  types: (.types | length),
  queries: (.types[] | select(.name == "Query") | .fields | length),
  mutations: (.types[] | select(.name == "Mutation") | .fields | length)
}' /etc/fraiseql/schema.compiled.json

# Check schema compiler source
ls -la *.schema.json 2>/dev/null || echo "No schema.json found in current directory"

# Check fraiseql.toml configuration
ls -la fraiseql.toml 2>/dev/null || echo "No fraiseql.toml found"
```

### 2. Compatibility Analysis

```bash
# Compare old and new schema
diff <(jq '.types | map(.name) | sort' /etc/fraiseql/schema.compiled.json.old) \
     <(jq '.types | map(.name) | sort' /etc/fraiseql/schema.compiled.json)

# Check for removed types (breaking change)
jq '.types[].name' /etc/fraiseql/schema.compiled.json.old | while read type; do
    if ! jq -e ".types[] | select(.name == \"$type\")" /etc/fraiseql/schema.compiled.json > /dev/null; then
        echo "✗ BREAKING: Type '$type' was removed"
    fi
done

# Check for removed fields (breaking change)
jq '.types[] | select(.fields != null) | {name: .name, fields: .fields[].name}' /etc/fraiseql/schema.compiled.json.old | \
  while read -r type; do
    # Compare fields...
done

# Check for permission/authorization changes
jq '.security' /etc/fraiseql/schema.compiled.json.old > /tmp/security_old.json
jq '.security' /etc/fraiseql/schema.compiled.json > /tmp/security_new.json
diff /tmp/security_old.json /tmp/security_new.json || echo "Security config changed"
```

### 3. Schema Compilation

```bash
# Check if schema source files exist
ls -la schema*.json fraiseql.toml 2>/dev/null || echo "No schema sources found"

# Verify fraiseql-cli is available
fraiseql-cli --version

# Dry-run compilation
fraiseql-cli compile schema.json --output /tmp/schema_test.compiled.json --dry-run

# Check compilation output
jq . /tmp/schema_test.compiled.json | head -20
```

### 4. Database Schema Impact

```bash
# Check if schema changes require database migrations
jq '.types[] | select(.database != null) | {name, database}' /etc/fraiseql/schema.compiled.json.old > /tmp/db_old.json
jq '.types[] | select(.database != null) | {name, database}' /etc/fraiseql/schema.compiled.json > /tmp/db_new.json
diff /tmp/db_old.json /tmp/db_new.json || echo "Database mappings unchanged"

# Verify database tables exist for schema
jq '.types[] | select(.database) | .database.table' /etc/fraiseql/schema.compiled.json | while read table; do
    psql $DATABASE_URL -c "SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name='$table')" 2>/dev/null
done
```

### 5. Client Compatibility

```bash
# Check schema changelog for breaking changes
jq '.metadata.changelog // []' /etc/fraiseql/schema.compiled.json | head -20

# List all queries (what clients depend on)
jq '.types[] | select(.name == "Query") | .fields[] | .name' /etc/fraiseql/schema.compiled.json

# List all mutations
jq '.types[] | select(.name == "Mutation") | .fields[] | .name' /etc/fraiseql/schema.compiled.json

# Check field deprecations
jq '.types[] | .fields[]? | select(.deprecated == true) | {type: .name, field: .name, reason: .deprecation_reason}' /etc/fraiseql/schema.compiled.json
```

## Mitigation

### Pre-Migration Validation

1. **Validate new schema**

   ```bash
   # Compile from source
   fraiseql-cli compile schema.json --output schema.compiled.json.new

   # Validate result
   jq empty schema.compiled.json.new && echo "✓ Schema valid" || echo "✗ Schema invalid"

   # Check size (huge increase may indicate error)
   ls -lh schema.compiled.json schema.compiled.json.new

   # Validate against current FraiseQL version
   fraiseql-cli validate schema.compiled.json.new
   ```

2. **Test in staging**

   ```bash
   # Deploy new schema to staging environment
   cp schema.compiled.json.new /etc/fraiseql-staging/schema.compiled.json

   # Restart staging server
   docker restart fraiseql-server-staging

   # Run smoke tests
   ./test/smoke-tests.sh http://localhost:8815-staging

   # Run integration tests
   cargo test --test integration_tests -- --test-threads=1

   # Monitor for errors
   docker logs fraiseql-server-staging | grep -i "error\|schema" | tail -20
   ```

3. **Identify breaking changes**

   ```bash
   # Compare schemas side-by-side
   python3 << 'EOF'
   import json

   with open('schema.compiled.json.old', 'r') as f:
       old_schema = json.load(f)
   with open('schema.compiled.json.new', 'r') as f:
       new_schema = json.load(f)

   # Find removed types
   old_types = {t['name'] for t in old_schema['types']}
   new_types = {t['name'] for t in new_schema['types']}
   removed = old_types - new_types
   added = new_types - old_types

   print(f"Removed types: {removed}")
   print(f"Added types: {added}")

   # Find removed fields per type
   for old_type in old_schema['types']:
       for new_type in new_schema['types']:
           if old_type['name'] == new_type['name']:
               if 'fields' in old_type and 'fields' in new_type:
                   old_fields = {f['name'] for f in old_type['fields']}
                   new_fields = {f['name'] for f in new_type['fields']}
                   removed_fields = old_fields - new_fields
                   if removed_fields:
                       print(f"Type {old_type['name']}: removed fields {removed_fields}")
   EOF
   ```

### Safe Migration Path

4. **Backup current schema**

   ```bash
   # Always backup before changes
   cp /etc/fraiseql/schema.compiled.json /etc/fraiseql/schema.compiled.json.backup-$(date +%Y%m%d-%H%M%S)

   # Verify backup
   ls -lah /etc/fraiseql/schema.compiled.json.backup-*
   ```

5. **Deploy new schema (non-breaking changes)**

   ```bash
   # If all changes are backward compatible (new fields/types only):

   # 1. Copy new schema
   cp schema.compiled.json.new /etc/fraiseql/schema.compiled.json

   # 2. Restart FraiseQL
   docker restart fraiseql-server
   sleep 5

   # 3. Verify
   curl http://localhost:8815/health | jq '.schema.version'

   # 4. Monitor error rate
   sleep 30
   curl -s http://localhost:8815/metrics | grep "errors_total"

   # 5. Rollback if needed
   if [ <error_rate> > 0.01 ]; then
       echo "Rolling back due to high error rate"
       cp /etc/fraiseql/schema.compiled.json.backup-* /etc/fraiseql/schema.compiled.json
       docker restart fraiseql-server
   fi
   ```

### Breaking Change Migration

6. **Coordinate breaking schema changes**

   ```bash
   # For breaking changes (removed fields/types):
   # Must coordinate with clients

   # 1. Deprecate old fields first (in previous schema version)
   # Deploy schema with deprecated fields marked
   # Give clients time to upgrade (usually 30-90 days)

   # 2. Then remove fields in next version
   # After deprecation period, deploy new schema without old fields

   # Example deprecation in schema:
   # "fields": [
   #   {
   #     "name": "oldField",
   #     "deprecated": true,
   #     "deprecation_reason": "Use newField instead (deprecated in v2.0, removed in v3.0)"
   #   }
   # ]

   # 3. Communicate with clients
   # Announce deprecation well in advance
   # Provide migration guide
   # Set clear removal date
   ```

## Resolution

### Complete Schema Migration Workflow

```bash
#!/bin/bash
set -e

echo "=== FraiseQL Schema Migration ==="

# Configuration
SCHEMA_SOURCE="schema.json"
OLD_SCHEMA="/etc/fraiseql/schema.compiled.json"
NEW_SCHEMA="/tmp/schema.compiled.json.new"
BACKUP_DIR="/etc/fraiseql/schema-backups"

# 1. Prepare
echo "1. Preparing migration..."
mkdir -p $BACKUP_DIR

# Backup current schema
TIMESTAMP=$(date +%Y%m%d-%H%M%S)
cp $OLD_SCHEMA $BACKUP_DIR/schema.compiled.json.$TIMESTAMP
echo "   ✓ Backup: $BACKUP_DIR/schema.compiled.json.$TIMESTAMP"

# 2. Compile new schema
echo ""
echo "2. Compiling new schema..."
if ! fraiseql-cli compile $SCHEMA_SOURCE --output $NEW_SCHEMA; then
    echo "   ✗ Compilation failed"
    exit 1
fi

# Validate
if ! jq empty $NEW_SCHEMA; then
    echo "   ✗ Output is not valid JSON"
    exit 1
fi
echo "   ✓ Schema compiled successfully"

# 3. Analyze changes
echo ""
echo "3. Analyzing changes..."

# Count types
OLD_TYPES=$(jq '.types | length' $OLD_SCHEMA)
NEW_TYPES=$(jq '.types | length' $NEW_SCHEMA)
echo "   Types: $OLD_TYPES → $NEW_TYPES"

# Check for breaking changes
python3 << 'EOF'
import json
import sys

with open(sys.argv[1], 'r') as f:
    old = json.load(f)
with open(sys.argv[2], 'r') as f:
    new = json.load(f)

old_type_names = {t['name'] for t in old['types']}
new_type_names = {t['name'] for t in new['types']}

removed = old_type_names - new_type_names
added = new_type_names - old_type_names

if removed:
    print(f"   ⚠ BREAKING: Removed types: {removed}")
else:
    print("   ✓ No types removed (not breaking)")

if added:
    print(f"   ✓ Added types: {added}")
EOF

# 4. Test in memory/staging
echo ""
echo "4. Testing new schema..."

# Verify database tables exist
echo "   Checking database tables..."
jq -r '.types[] | select(.database != null) | .database.table' $NEW_SCHEMA 2>/dev/null | while read table; do
    if psql $DATABASE_URL -c "SELECT 1 FROM information_schema.tables WHERE table_name='$table'" 2>/dev/null | grep -q 1; then
        echo "      ✓ Table $table exists"
    else
        echo "      ✗ Table $table missing"
    fi
done

# 5. Validation
echo ""
echo "5. Validating schema..."
if fraiseql-cli validate $NEW_SCHEMA; then
    echo "   ✓ Schema validation passed"
else
    echo "   ✗ Schema validation failed"
    exit 1
fi

# 6. Get approval for breaking changes
if python3 << 'EOF' 2>/dev/null; $(jq '.metadata.breaking_changes // false' $NEW_SCHEMA | grep -q true); then
    echo ""
    echo "⚠ WARNING: This migration contains breaking changes"
    echo "   Have clients been notified and prepared for this change?"
    read -p "   Continue? (yes/no): " -r
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        echo "   Aborted"
        exit 1
    fi
fi

# 7. Deploy
echo ""
echo "6. Deploying new schema..."
cp $NEW_SCHEMA $OLD_SCHEMA
echo "   ✓ Schema updated"

# 8. Restart server
echo ""
echo "7. Restarting FraiseQL..."
docker restart fraiseql-server
sleep 5
echo "   ✓ Server restarted"

# 9. Verify
echo ""
echo "8. Verification..."

# Health check
if ! curl -s http://localhost:8815/health | jq -e '.status == "healthy"' > /dev/null; then
    echo "   ✗ Health check failed"
    echo "   Rolling back..."
    cp $BACKUP_DIR/schema.compiled.json.$TIMESTAMP $OLD_SCHEMA
    docker restart fraiseql-server
    exit 1
fi
echo "   ✓ Server healthy"

# Schema version
NEW_VERSION=$(jq '.metadata.version' $OLD_SCHEMA)
echo "   ✓ New schema version: $NEW_VERSION"

# Check error rate
sleep 10
ERROR_RATE=$(curl -s http://localhost:8815/metrics | grep "request_errors_total" | awk '{print $NF}' || echo "0")
echo "   Error rate: $ERROR_RATE"

echo ""
echo "✓ Schema migration complete"
echo "  Previous schema backed up: $BACKUP_DIR/schema.compiled.json.$TIMESTAMP"
```

### Rollback Procedure

```bash
#!/bin/bash

echo "=== Rolling Back Schema ==="

# Find latest backup
LATEST_BACKUP=$(ls -t /etc/fraiseql/schema-backups/schema.compiled.json.* | head -1)

if [ -z "$LATEST_BACKUP" ]; then
    echo "✗ No backup found"
    exit 1
fi

echo "Rolling back to: $LATEST_BACKUP"

# Restore
cp "$LATEST_BACKUP" /etc/fraiseql/schema.compiled.json

# Restart
docker restart fraiseql-server
sleep 5

# Verify
if curl -s http://localhost:8815/health | jq -e '.status == "healthy"' > /dev/null; then
    echo "✓ Rollback successful"
else
    echo "✗ Rollback verification failed"
    exit 1
fi
```

## Prevention

### Schema Management Best Practices

- **Version control**: Keep schema.json in git with clear versioning
- **Changelog**: Document all changes in schema metadata
- **Deprecation policy**: Mark deprecated fields 2-3 releases before removal
- **Breaking change communication**: Announce far in advance (30-90 days)
- **Staging environment**: Always test new schema in staging first
- **Client coordination**: Sync schema updates with client team
- **Backward compatibility**: Prefer adding fields over removing them

### Schema Review Process

```bash
# Before deploying new schema:

# 1. Code review
# Have team review schema.json changes

# 2. Automated validation
fraiseql-cli validate schema.json

# 3. Database compatibility
# Verify all referenced tables exist in database

# 4. Client compatibility
# Check with client team on any breaking changes

# 5. Staging deployment
# Deploy to staging, run tests, monitor

# 6. Documentation
# Update API documentation reflecting new schema

# 7. Change announcement
# If breaking changes, announce with migration guide
```

## Escalation

- **Schema compilation errors**: Schema/compiler team
- **Database compatibility issues**: Database team
- **Client compatibility issues**: Client / Integration team
- **Breaking change coordination**: Product / API team
- **Urgent schema rollback**: On-call engineer
