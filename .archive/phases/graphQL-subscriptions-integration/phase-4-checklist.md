# Phase 3 Implementation Checklist

**Phase**: 3 - Python High-Level API
**Engineer**: Junior Python Web Framework Developer
**Timeline**: 3 weeks / 30 hours

---

## Pre-Implementation Checklist

- [ ] Phase 2 complete (event dispatcher working)
- [ ] Read `phase-3.md` implementation plan
- [ ] Understand WebSocketAdapter abstraction
- [ ] Check FastAPI and Starlette WebSocket APIs
- [ ] Review GraphQL Transport WS protocol

---

## Task 3.0: HTTP Abstraction Layer

### Requirements
- [ ] Create WebSocketAdapter ABC
- [ ] Implement FastAPIWebSocketAdapter
- [ ] Implement StarletteWebSocketAdapter
- [ ] Create SubscriptionProtocolHandler ABC
- [ ] Implement GraphQLTransportWSHandler

### WebSocketAdapter ABC Checklist
- [ ] `accept(subprotocol)` method defined
- [ ] `receive_json()` method defined
- [ ] `send_json(data)` method defined
- [ ] `send_bytes(data)` method defined (critical for performance)
- [ ] `close(code, reason)` method defined
- [ ] `is_connected` property defined

### FastAPI Adapter Checklist
- [ ] Wraps FastAPI WebSocket correctly
- [ ] All 6 methods implemented
- [ ] Error handling for WebSocket state
- [ ] Proper async/await usage

### Starlette Adapter Checklist
- [ ] Wraps Starlette WebSocket correctly
- [ ] `receive_json()` implemented (Starlette lacks this)
- [ ] All 6 methods implemented
- [ ] Compatible with Starlette WebSocket API

### Protocol Handler Checklist
- [ ] SubscriptionProtocolHandler ABC defined
- [ ] GraphQLTransportWSHandler implements protocol
- [ ] Connection lifecycle handled (init, subscribe, complete)
- [ ] Error handling and cleanup
- [ ] Listener tasks managed properly

---

## Task 3.1: Framework-Agnostic SubscriptionManager

### Requirements
- [ ] Create SubscriptionManager class
- [ ] Implement all user-facing methods
- [ ] Store subscription metadata in Python
- [ ] Handle resolver function mapping
- [ ] Zero framework-specific code

### Core Methods Checklist
- [ ] `__init__()` with EventBusConfig
- [ ] `create_subscription()` - register with Rust + store metadata
- [ ] `publish_event()` - delegate to Rust
- [ ] `get_next_event()` - get bytes from Rust
- [ ] `complete_subscription()` - cleanup both Python and Rust
- [ ] `get_metrics()` - return metrics dict

### Resolver Management Checklist
- [ ] `register_resolver()` method
- [ ] `get_resolver()` method
- [ ] Resolver lookup in protocol handler
- [ ] Error handling for missing resolvers

### Framework Independence Checklist
- [ ] No FastAPI imports
- [ ] No Starlette imports
- [ ] No WebSocket-specific code
- [ ] Pure Python business logic layer

---

## Task 3.2: Framework-Specific Integrations

### FastAPI Integration Checklist
- [ ] SubscriptionRouterFactory class created
- [ ] `create()` static method implemented
- [ ] FastAPI router creation
- [ ] WebSocket endpoint registration
- [ ] Protocol handler integration
- [ ] Auth handler support

### Starlette Integration Checklist
- [ ] `create_subscription_app()` function
- [ ] Starlette route creation
- [ ] WebSocket endpoint registration
- [ ] Protocol handler integration
- [ ] Auth handler support

### Custom Server Template Checklist
- [ ] CustomServerWebSocketAdapter example
- [ ] All 6 methods implemented
- [ ] Integration instructions
- [ ] Error handling examples

---

## Integration Testing

### Unit Tests
- [ ] WebSocketAdapter implementations work
- [ ] Protocol handler handles messages correctly
- [ ] SubscriptionManager methods functional
- [ ] Framework routers created successfully

### Framework Integration Tests
- [ ] FastAPI router integrates with SubscriptionManager
- [ ] Starlette app integrates with SubscriptionManager
- [ ] Custom adapter follows interface contract

### End-to-End Protocol Test
- [ ] Mock WebSocketAdapter for testing
- [ ] Test connection_init message
- [ ] Test subscribe message
- [ ] Test complete message
- [ ] Test error handling

---

## Phase 3 Verification

### Imports & Instantiation
- [ ] All new modules import without errors
- [ ] SubscriptionManager creates successfully
- [ ] Framework factories work
- [ ] No circular import issues

### FastAPI Integration Test
```python
from fraiseql.subscriptions import SubscriptionManager
from fraiseql.integrations.fastapi_subscriptions import SubscriptionRouterFactory
from fastapi import FastAPI

manager = SubscriptionManager(config)
router = SubscriptionRouterFactory.create(manager)
app = FastAPI()
app.include_router(router)
# Should work without errors
```

### Starlette Integration Test
```python
from fraiseql.integrations.starlette_subscriptions import create_subscription_app
from starlette.applications import Starlette

app = Starlette()
manager = SubscriptionManager(config)
create_subscription_app(app, manager)
# Should work without errors
```

### Protocol Handler Test
- [ ] Handles graphql-transport-ws messages
- [ ] Manages subscription lifecycle
- [ ] Sends correct response format
- [ ] Cleans up on disconnect

---

## Phase 3 Success Criteria Met

- [ ] ✅ HTTP abstraction layer complete
- [ ] ✅ WebSocketAdapter implementations working
- [ ] ✅ GraphQLTransportWSHandler implements protocol
- [ ] ✅ SubscriptionManager framework-agnostic
- [ ] ✅ FastAPI integration complete
- [ ] ✅ Starlette integration complete
- [ ] ✅ Custom server template provided
- [ ] ✅ All unit tests pass
- [ ] ✅ Type checking clean

---

## Next Steps

Once Phase 3 is complete:
1. **Commit changes** with message: `feat: Phase 3 - Python high-level API with HTTP abstraction`
2. **Update project status** to Phase 3 ✅ Complete
3. **Start Phase 4** - Integration & testing
4. **Notify team** that Phase 3 is ready for review

---

## Help Resources

- **Reference Code**: Existing framework integrations in FraiseQL
- **Planning Docs**: `SUBSCRIPTIONS_INTEGRATION_PLAN_V3_HTTP_ABSTRACTION.md`
- **Protocol**: GraphQL Transport WS specification
- **Senior Help**: For framework-specific WebSocket APIs or protocol implementation

---

**Phase 3 Checklist Complete**: Ready for implementation</content>
<parameter name="filePath">/home/lionel/code/fraiseql/.phases/graphQL-subscriptions-integration/phase-3-checklist.md
