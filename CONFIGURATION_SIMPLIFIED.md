# CamelForge Configuration Simplified

## **Before: Too Complex** ❌

```python
# Multiple conflicting configuration sources
config = FraiseQLConfig(
    camelforge_enabled=True,              # Config setting
    camelforge_function="turbo.fn_camelforge",
    camelforge_entity_mapping=True,       # Extra complexity
    enable_feature_flags=True,            # Extra layer
    feature_flags_source="environment",   # Extra complexity
)

# PLUS environment variables
FRAISEQL_CAMELFORGE_BETA=true            # Beta flag?
FRAISEQL_CAMELFORGE_DEBUG=true           # Debug flag?
FRAISEQL_CAMELFORGE_ALLOWLIST=dns_server # Entity restrictions?
FRAISEQL_CAMELFORGE_BLOCKLIST=contract   # More restrictions?
FRAISEQL_CAMELFORGE_SAFE_MODE=true       # Safe mode?

# PLUS feature flags
from fraiseql.fastapi.feature_flags import FeatureFlags
flags = FeatureFlags.from_environment()
if flags.should_use_camelforge("dns_server"):  # Complex logic
    # Do something
```

**Problems:**
- Too many configuration sources
- Conflicting precedence rules
- Complex testing setup
- Hard to understand which setting actually controls behavior

## **After: Simple & Clear** ✅

```python
# Single, clear configuration
config = FraiseQLConfig(
    database_url="postgresql://...",
    camelforge_enabled=True,                    # Simple on/off
    camelforge_function="turbo.fn_camelforge",  # Optional
    camelforge_field_threshold=20,              # Optional
)
```

**Or with environment variables:**
```bash
# Simple environment override
export FRAISEQL_CAMELFORGE_ENABLED=true
export FRAISEQL_CAMELFORGE_FUNCTION=turbo.fn_camelforge    # Optional
export FRAISEQL_CAMELFORGE_FIELD_THRESHOLD=30             # Optional

python your_app.py
```

## **Configuration Precedence (Simple)**

1. **Environment Variables** (highest priority) - for testing/overrides
2. **Config Parameters** - for application defaults
3. **Built-in Defaults** - for fallback

```python
# Example: Config says disabled, environment says enabled
config = FraiseQLConfig(
    camelforge_enabled=False,  # Config setting
)

# Environment variable overrides config
# FRAISEQL_CAMELFORGE_ENABLED=true
# Result: CamelForge will be enabled
```

## **Usage Examples**

### **Production Setup**
```python
config = FraiseQLConfig(
    database_url=DATABASE_URL,
    camelforge_enabled=True,  # Enable for production
    camelforge_field_threshold=30,  # Tune for your queries
)
```

### **Development Setup**
```python
config = FraiseQLConfig(
    database_url=DATABASE_URL,
    camelforge_enabled=False,  # Disabled by default
)

# Enable via environment when needed
# FRAISEQL_CAMELFORGE_ENABLED=true python dev_server.py
```

### **Testing Setup**
```bash
# Test without CamelForge
FRAISEQL_CAMELFORGE_ENABLED=false npm test

# Test with CamelForge
FRAISEQL_CAMELFORGE_ENABLED=true npm test
```

## **What Got Removed**

❌ **Removed Complex Features:**
- Beta flags (`FRAISEQL_CAMELFORGE_BETA`)
- Debug flags (`FRAISEQL_CAMELFORGE_DEBUG`)
- Safe mode flags (`FRAISEQL_CAMELFORGE_SAFE_MODE`)
- Entity allowlists (`FRAISEQL_CAMELFORGE_ALLOWLIST`)
- Entity blocklists (`FRAISEQL_CAMELFORGE_BLOCKLIST`)
- Feature flag system (`FeatureFlags` class)
- Auto-mapping configuration (`camelforge_entity_mapping`)

❌ **Removed Complex Files:**
- `feature_flags.py`
- Complex testing documentation
- Beta-specific configuration layers

## **What Stayed**

✅ **Kept Essential Features:**
- `camelforge_enabled` - Simple on/off switch
- `camelforge_function` - Function name customization
- `camelforge_field_threshold` - Performance tuning
- Environment variable overrides
- Automatic entity type derivation (simplified)
- All core CamelForge functionality

## **Migration Guide**

### **From Beta Configuration:**
```bash
# Old (complex)
export FRAISEQL_CAMELFORGE_BETA=true
export FRAISEQL_CAMELFORGE_DEBUG=true
export FRAISEQL_CAMELFORGE_ALLOWLIST=dns_server

# New (simple)
export FRAISEQL_CAMELFORGE_ENABLED=true
```

### **From Complex Config:**
```python
# Old (complex)
config = FraiseQLConfig(
    camelforge_enabled=True,
    camelforge_entity_mapping=True,
    enable_feature_flags=True,
    feature_flags_source="environment",
)

# New (simple)
config = FraiseQLConfig(
    camelforge_enabled=True,
)
```

## **Benefits of Simplified Approach**

✅ **Clarity**: One setting controls CamelForge
✅ **Simplicity**: No conflicting configuration sources
✅ **Testability**: Easy to enable/disable for testing
✅ **Maintainability**: Less code, fewer edge cases
✅ **User-Friendly**: Clear documentation, simple examples

## **Testing Is Now Simple**

```bash
# Enable CamelForge and test
FRAISEQL_CAMELFORGE_ENABLED=true python -m pytest

# That's it!
```

The configuration is now as simple as setting one environment variable, while still providing the flexibility to customize function names and thresholds when needed.
