# Phase A Post-Mortem: What Went Wrong

**Date:** 2026-01-30
**Incident:** Implemented wrong architecture for subscription event capture
**Commit Reverted:** e6a5ed57 "feat(subscriptions): Wire PostgresListener into server startup"

---

## Executive Summary

I implemented a PostgreSQL NOTIFY/LISTEN based event capture system for subscriptions (Phase A) that:
- ❌ Duplicated existing event infrastructure (ChangeLogListener)
- ❌ Violated FraiseQL's database-centric design philosophy
- ❌ Added unnecessary complexity (new listener, config, startup logic)
- ❌ Created two competing event systems instead of one unified pipeline

The correct approach is to integrate SubscriptionManager into the existing ObserverRuntime event pipeline, using tb_entity_change_log as the single source of truth.

---

## What I Did Wrong

### 1. Didn't Understand the Core Architecture

**What I Thought:**
- Subscriptions need "real-time" → therefore need NOTIFY/LISTEN
- Observers are "batch-oriented" → therefore need polling
- These are fundamentally different systems

**Reality:**
- FraiseQL is **database-centric** - everything flows through database tables
- `tb_entity_change_log` is THE event log (single source of truth)
- ChangeLogListener already polls it every 100ms (effectively real-time)
- Observers and subscriptions should share the same event pipeline

**Evidence I Missed:**
```rust
// crates/fraiseql-observers/src/listener/change_log.rs
pub struct ChangeLogListener {
    config: ChangeLogListenerConfig,
    pool: PgPool,
    checkpoint: Option<i64>,
}

impl ChangeLogListener {
    pub async fn next_batch(&mut self) -> Result<Vec<ChangeLogEntry>> {
        // Polls tb_entity_change_log with checkpoint tracking
        // This is ALREADY a real-time event stream (100ms polling)
    }
}
```

I should have asked: "Why do we need NOTIFY when we already poll tb_entity_change_log every 100ms?"

---

### 2. Followed Documentation Without Critical Analysis

**What Happened:**
- Read `docs/architecture/realtime/subscriptions.md`
- It described LISTEN/NOTIFY architecture
- I implemented exactly what it said

**What I Should Have Done:**
- Compare documentation against actual codebase
- Check if ObserverRuntime already provides event infrastructure
- Question why two separate event capture mechanisms exist
- Validate that documented design matches actual architecture

**The Documentation Was Wrong (or outdated):**
```markdown
# From docs/architecture/realtime/subscriptions.md (lines 99-102)
┌───────────────────┐            ┌──────────────────────┐
│ LISTEN / NOTIFY   │            │ CDC (Change Data     │
│ (Low-latency)     │            │  Capture)            │
```

This diagram shows LISTEN/NOTIFY as a separate event source, but in reality:
- No CDC integration exists
- Only ChangeLogListener polling exists
- LISTEN/NOTIFY was never implemented
- Documentation described aspirational architecture, not actual

---

### 3. Didn't Search for Existing Event Infrastructure

**What I Did:**
1. Read subscription.rs code (PostgresListener implementation exists)
2. Assumed it needs to be wired up
3. Created server config, startup logic, shutdown handlers

**What I Should Have Done:**
1. Search for "ChangeLogListener" across codebase
2. Trace how ObserverRuntime processes events
3. Realize SubscriptionManager and ObserverExecutor need the SAME events
4. Ask: "Can we route ChangeLogListener events to both systems?"

**The Smoking Gun:**
```rust
// crates/fraiseql-server/src/observers/runtime.rs (lines 291-468)
let handle = tokio::spawn(async move {
    let mut listener = ChangeLogListener::new(listener_config);
    loop {
        tokio::select! {
            result = listener.next_batch() => {
                match result {
                    Ok(entries) => {
                        for entry in entries {
                            let event = EntityEvent::from_change_log_entry(entry);

                            // THIS is where events are processed!
                            // I should have added SubscriptionManager HERE
                            executor.process_event(&event).await;
                        }
                    }
                }
            }
        }
    }
});
```

I missed this entire background task loop that's already processing database events!

---

### 4. Added Complexity Instead of Reusing

**What I Implemented:**

| Component | Lines of Code | Purpose |
|-----------|---------------|---------|
| SubscriptionListenerConfig | 40 lines | Config for NOTIFY listener |
| Server.subscription_listener_handle | 10 lines | Storage for handle |
| Listener startup logic | 50 lines | Initialize and start listener |
| Graceful shutdown logic | 20 lines | Stop listener on shutdown |
| **Total** | **~120 lines** | Duplicate event infrastructure |

**What I Should Have Done:**

| Component | Lines of Code | Purpose |
|-----------|---------------|---------|
| ObserverRuntime.subscription_manager | 5 lines | Add Arc<SubscriptionManager> field |
| Publish to subscriptions | 10 lines | Add subscription_manager.publish_event() in loop |
| **Total** | **~15 lines** | Reuse existing infrastructure |

**The Comparison:**
- My approach: 120 lines, new config, new listener, new shutdown logic
- Correct approach: 15 lines, extend existing ObserverRuntime

---

### 5. Ignored the "Database-Centric" Design Principle

**From .claude/CLAUDE.md (Core Architecture Principle):**
```
Authoring (Python/TS) → Compilation (Rust) → Runtime (Rust)
         ↓                      ↓                    ↓
   schema.json        schema.compiled.json    GraphQL Server
```

**Key Point from docs/foundation/03-database-centric-architecture.md:**
> "Database as primary interface (GraphQL as DB access layer, not aggregation)"

FraiseQL's philosophy:
- Database is the source of truth
- All data flows through database tables
- No external event buses or message queues
- Polling database tables is the PRIMARY pattern

**I violated this by:**
- Adding NOTIFY/LISTEN (external to table polling)
- Creating parallel event capture mechanism
- Treating PostgreSQL NOTIFY as "better than polling"

In FraiseQL's world:
- `tb_entity_change_log` is already "real-time" (100ms polling)
- Adding NOTIFY doesn't meaningfully improve latency
- Database tables > message channels

---

## The Correct Architecture

### Event Flow (Unified)

```
Mutation Execution
    ↓
INSERT INTO tb_entity_change_log (object_type, object_id, modification_type, object_data)
    ↓ (single source of truth)
ChangeLogListener.next_batch() (polls every 100ms)
    ↓
ChangeLogEntry → EntityEvent conversion
    ↓
ObserverRuntime background task
    ├─ ObserverExecutor.process_event() → Actions (webhook, email, etc.)
    └─ SubscriptionManager.publish_event() → Transports (WebSocket, Kafka)
```

### Implementation (15 lines)

```rust
// In ObserverRuntime struct
pub struct ObserverRuntime {
    // ... existing fields ...
    subscription_manager: Option<Arc<SubscriptionManager>>,  // +1 line
}

// In ObserverRuntime::new()
pub fn new(
    config: ObserverRuntimeConfig,
    subscription_manager: Option<Arc<SubscriptionManager>>,  // +1 line
) -> Self {
    Self {
        // ... existing fields ...
        subscription_manager,  // +1 line
    }
}

// In background task loop (line ~420)
for entry in entries {
    let event = EntityEvent::from_change_log_entry(entry);

    // Route to observers (existing)
    executor.process_event(&event).await;

    // Route to subscriptions (NEW - 5 lines)
    if let Some(ref sub_manager) = subscription_manager {
        if let Err(e) = sub_manager.publish_event(&event).await {
            warn!("Failed to publish to subscriptions: {}", e);
        }
    }
}
```

**Total change:** ~15 lines, zero new infrastructure.

---

## Root Cause Analysis

### Primary Cause: Insufficient Architectural Understanding

**Timeline of Mistakes:**
1. User asked to align documentation with reality
2. I discovered subscription code exists but isn't wired up
3. I created tasks for "Phase A: Wire PostgresListener"
4. User approved "ok, let's implement then"
5. **I never questioned if PostgresListener was the right approach**

**What I Should Have Done:**
1. Discover subscription code exists
2. **Ask:** "Why does ObserverRuntime exist? What does it do?"
3. **Realize:** Observers already process database events
4. **Propose:** "Should we integrate subscriptions into observer pipeline?"
5. **Get approval** before implementing

### Contributing Factors

**1. Task Framing Bias**
- Tasks were named "Phase A/B/C/D" with predefined scope
- I felt committed to the plan
- Didn't stop to re-evaluate when evidence contradicted it

**2. Code Exists = Must Be Used**
- PostgresListener implementation exists in subscription.rs
- I assumed "it exists, therefore it should be wired up"
- Didn't consider it might be dead code or wrong design

**3. Documentation Trust**
- docs/architecture/realtime/subscriptions.md described NOTIFY architecture
- I trusted it without validating against codebase
- Documentation was aspirational, not actual

**4. Pattern Matching Instead of First Principles**
- Saw "ObserverRuntime starts listener in serve()"
- Copied pattern for "SubscriptionListener starts in serve()"
- Didn't ask "why do we need TWO listeners?"

---

## Lessons Learned

### For Future Architecture Work

1. **Always trace data flow end-to-end before implementing**
   - Where does data enter? (tb_entity_change_log)
   - How is it processed? (ChangeLogListener → ObserverRuntime)
   - Where does it go? (ObserverExecutor actions)
   - Can new features reuse this pipeline? (YES!)

2. **Question documentation that conflicts with code**
   - Docs said: LISTEN/NOTIFY for subscriptions
   - Code showed: ChangeLogListener polling for observers
   - Conflict → investigate which is correct

3. **Search for "similar existing functionality" before building new**
   - Before implementing PostgresListener, should have searched for:
     - "listener" in codebase
     - "event" processing
     - "background task" or "spawn"
   - Would have found ObserverRuntime immediately

4. **Resist the "task list" pressure**
   - Just because tasks are written doesn't mean they're correct
   - Re-evaluate when new information appears
   - It's okay to say "Phase A is wrong, let me rethink"

5. **Understand design principles FIRST**
   - FraiseQL is database-centric
   - This means: database tables > message channels
   - If I'd internalized this, I'd have questioned NOTIFY immediately

### For Working With Users

1. **Present options, not implementations**
   - Instead of: "I'll wire up PostgresListener"
   - Should say: "I found two event systems. Should we unify them or keep separate?"

2. **Explain trade-offs before committing**
   - Option 1: PostgresListener (new infrastructure, duplicate logic)
   - Option 2: Extend ObserverRuntime (reuse existing, simpler)
   - Let user choose architecture direction

3. **Ask clarifying questions early**
   - "Do we already have event infrastructure?" (YES - ObserverRuntime)
   - "Why does PostgresListener exist if not wired up?" (Dead code? Wrong design?)
   - "Can subscriptions reuse observer events?" (YES!)

---

## Prevention Checklist

Before implementing any "integration" or "wiring" task:

- [ ] Trace the full data flow in the existing system
- [ ] Search for similar functionality already implemented
- [ ] Compare documentation against actual code
- [ ] Understand the core design philosophy (e.g., "database-centric")
- [ ] Consider reusing existing infrastructure before building new
- [ ] Present architecture options to user before implementing
- [ ] Question if "code exists" implies "code should be used"
- [ ] Validate that the plan still makes sense with new information

---

## Corrected Plan

### Phase A (Revised): Integrate SubscriptionManager into ObserverRuntime

**Goal:** Route tb_entity_change_log events to both observers AND subscriptions

**Changes Required:**
1. Add `Arc<SubscriptionManager>` to ObserverRuntime
2. Pass it from Server::new() → init_observer_runtime()
3. In background task loop: publish events to subscription_manager
4. Convert EntityEvent format to SubscriptionEvent format

**Estimated Effort:** ~30 minutes (vs. 2+ hours for Phase A)

### Phase B (Revised): Add mutation hooks to populate tb_entity_change_log

**Goal:** Automatically INSERT into tb_entity_change_log after mutations

**Options:**
1. **Database triggers** - Pure SQL, automatic
2. **Executor hooks** - Rust code after mutation execution
3. **Application explicit** - Keep current manual pattern

**Recommendation:** Option 2 (Executor hooks) - fits FraiseQL's compiled architecture

### Phase C/D: Unchanged
- Multi-tenant authorization
- End-to-end tests

---

## Final Thoughts

This was a valuable learning experience. The key insight:

> **When you find duplicate infrastructure, the answer is almost never "wire up both." It's "unify into one."**

PostgresListener and ChangeLogListener were solving the same problem (capture database events) in different ways. The correct solution was to pick one (ChangeLogListener, because it's database-centric) and extend it to serve both use cases.

I should have recognized this pattern immediately, but instead I assumed the duplication was intentional and tried to "complete" both systems.

**The user's question "do we really need database triggers?" was the right instinct.** They sensed something was off about the approach. I should have paused and re-evaluated the entire plan at that point.

---

## Recommendations for Codebase

1. **Remove PostgresListener code from subscription.rs**
   - It's unused and misleading
   - Suggests NOTIFY architecture that doesn't match reality
   - ~270 lines of dead code

2. **Update docs/architecture/realtime/subscriptions.md**
   - Remove LISTEN/NOTIFY diagrams
   - Document actual architecture: tb_entity_change_log → ChangeLogListener → both systems
   - Be honest about what's implemented vs. aspirational

3. **Add architecture decision record (ADR)**
   - Document why subscriptions use polling, not NOTIFY
   - Explain database-centric philosophy
   - Guide future developers away from this mistake

---

**Status:** Phase A reverted, architecture re-evaluated, ready to implement correct approach.

**Next:** Implement Phase A (revised) - Integrate SubscriptionManager into ObserverRuntime
