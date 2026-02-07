---
parent: Decisions
nav_order: 10
# These are optional metadata elements. Feel free to remove any of them.
status: "proposed"
date: 2026-02-07
decision-makers:
    - abasu
---
# Use hybrid routing with static configuration and dynamic learning

## Context and Problem Statement

RINA Inter-Process Communication Processes require route resolution to determine next-hop addresses for packet forwarding to remote destinations. Some routes are known at deployment time (static configuration from TOML files), whilst others must be learned dynamically during runtime (enrolment of new members, topology changes). Additionally, dynamically learned routes may become stale if members disconnect or network topology changes, requiring expiration mechanisms. We need a routing approach that combines the predictability of static configuration with the flexibility of dynamic learning whilst preventing stale route accumulation.

## Considered Options

* Use hybrid routing with static and dynamic routes with Time-To-Live (TTL)-based expiration and persistence.
* Use purely static routing with all routes pre-configured before IPCP startup.
* Use purely dynamic routing with all routes learned at runtime without persistence.
* Use centralised routing service with external route distribution protocol.

## Decision Outcome

Chosen option: "Use hybrid routing with static and dynamic routes with TTL-based expiration and persistence", because it provides operational flexibility by supporting both pre-configured routes (for known infrastructure) and runtime-learned routes (for dynamic membership changes). Static routes offer deterministic forwarding for critical paths, whilst dynamic routes enable automatic discovery during enrolment. TTL-based expiration prevents stale route accumulation, and optional TOML persistence enables recovery of learned routes after IPCP restarts. The `RouteResolver` abstraction in [routing.rs](src/routing.rs) implements this hybrid approach with a clear priority hierarchy: static routes take precedence over dynamic routes, ensuring configuration always overrides runtime learning.

## Pros and Cons of the Options

### Use hybrid routing with static and dynamic routes with Time-To-Live (TTL)-based expiration and persistence

* Good, because static routes provide deterministic forwarding for known infrastructure without runtime discovery overhead.
* Good, because dynamic routes enable automatic topology adaptation during enrolment and membership changes.
* Good, because lookup priority (static → dynamic) ensures configuration always overrides runtime learning.
* Good, because TTL-based expiration automatically removes stale routes without manual intervention (default 3600 seconds).
* Good, because TOML persistence (`RouteSnapshot`) enables recovery of learned routes after IPCP restarts.
* Good, because TTL validation on snapshot load filters expired routes, preventing stale state restoration.
* Good, because periodic snapshots (configurable interval, default 300 seconds) ensure recent dynamic routes persist across failures.
* Good, because `RouteResolver` abstraction decouples route management from RMT forwarding logic (ADR 0008).
* Good, because idempotent route addition handles re-enrolment scenarios (updating existing routes rather than failing).
* Neutral, because it requires managing two route sources (static via RIB, dynamic via metadata cache), though this is cleanly abstracted.
* Bad, because TTL management adds complexity (expiration checking, remaining TTL calculation).
* Bad, because snapshot persistence requires file I/O, though this occurs asynchronously in background tasks.

### Use purely static routing with all routes pre-configured before IPCP startup

* Good, because it provides maximum determinism—all routes known before operation begins.
* Good, because it eliminates runtime route learning complexity and TTL management.
* Good, because static configuration is simple to reason about and troubleshoot.
* Neutral, because operators must know complete topology at deployment time.
* Bad, because it cannot adapt to membership changes—new members require IPCP restarts and configuration updates.
* Bad, because it does not scale to large, dynamic DIFs where topology frequently changes.
* Bad, because enrolment of new members cannot automatically create routes, requiring manual intervention.
* Bad, because it violates RINA's principle of dynamic membership where IPCPs enrol and de-enrol at runtime.

### Use purely dynamic routing with all routes learned at runtime without persistence

* Good, because it requires no pre-configuration—all routes discovered automatically.
* Good, because it adapts to topology changes without operator intervention.
* Good, because it simplifies deployment (no route configuration files).
* Neutral, because route learning protocols (link-state, distance-vector) must be implemented.
* Bad, because it lacks determinism—initial forwarding depends on runtime discovery timing.
* Bad, because no persistence means all routes lost on IPCP restart, requiring full rediscovery.
* Bad, because it cannot express operator intent (preferred paths, cost constraints) available in static configuration.
* Bad, because bootstrap IPCPs have no initial routes to reach first enrolling members.

### Use centralised routing service with external route distribution protocol

* Good, because it provides consistent routing decisions across all IPCPs in a DIF.
* Good, because centralised computation can implement global optimisation (traffic engineering, load balancing).
* Good, because it simplifies individual IPCP logic—fetch routes from central service.
* Neutral, because it requires implementing route distribution protocol (e.g., RINA-specific or adapted BGP).
* Bad, because it introduces external dependency violating RINA's self-sufficiency principle.
* Bad, because centralised service becomes single point of failure for routing decisions.
* Bad, because it adds deployment complexity (separate routing service process, coordination protocol).
* Bad, because it contradicts RINA's distributed architecture where IPCPs operate autonomously.

## More Information

### Current Implementation

`RouteResolver` in [src/routing.rs](src/routing.rs) implements hybrid routing with:

#### Route Storage

* **Static routes**: Stored in RIB at `/routing/static/{destination}` as `RibValue::Struct` with `next_hop_address` and `next_hop_rina_addr` fields (populated from TOML configuration).
* **Dynamic routes**: Stored in RIB at `/routing/dynamic/{destination}` with same structure, plus metadata cache (`HashMap<u64, RouteMetadata>`) tracking TTL and creation time.

#### Route Lookup Priority

`resolve_next_hop(dst_addr)` implements ordered lookup:

1. **Static routes** (highest priority): Query RIB `/routing/static/{dst_addr}`
2. **Dynamic routes** (with TTL check): Query RIB `/routing/dynamic/{dst_addr}`, verify not expired via metadata cache
3. **Not found**: Return `RouteNotFound` error

#### TTL Management

* `RouteMetadata`: Stores `destination`, `next_hop_address`, `created_at` (Unix timestamp), `ttl_seconds` (0 = never expires).
* `is_expired()`: Checks current time against `created_at + ttl_seconds`.
* `remaining_ttl()`: Calculates seconds until expiration for snapshot persistence.
* Automatic removal: Expired routes deleted during lookup, triggering `RouteNotFound`.

#### Persistence

* **Snapshot format**: TOML serialisation of `RouteSnapshot` with version, timestamp, and `Vec<RouteMetadata>`.
* **Save**: `save_snapshot()` writes metadata cache to TOML file, creating parent directories if needed.
* **Load**: `load_snapshot()` reads TOML, filters expired routes via `filter_valid()`, restores valid routes with remaining TTL.
* **Immediate save**: Route additions trigger immediate snapshot if persistence enabled.
* **Periodic snapshots**: `start_snapshot_task()` spawns background tokio task saving at configured interval (default 300 seconds).

#### Dynamic Route Management

* **Add**: `add_dynamic_route(dst_addr, next_hop, ttl)` creates or updates RIB entry, stores metadata, triggers immediate snapshot (idempotent for re-enrolment).
* **Remove**: `remove_dynamic_route(dst_addr)` deletes RIB entry and metadata (called on expiration or explicit disconnection).
* **Bootstrap usage**: Enrolment manager calls `add_dynamic_route()` when accepting member enrolment, creating route to member's allocated address.

### Design Rationale

* **Hybrid approach**: Balances operator control (static routes) with runtime flexibility (dynamic learning).
* **Static priority**: Ensures configuration overrides runtime learning, enabling operators to enforce preferred paths.
* **TTL expiration**: Prevents stale route accumulation without requiring explicit cleanup protocols.
* **Persistence**: Enables fast recovery after restarts whilst filtering expired state.
* **RIB integration**: Leverages existing distributed object management (ADR 0004) rather than separate routing tables.

### Conclusion

We choose hybrid routing with static configuration and dynamic learning because it provides the flexibility required for RINA's dynamic membership model whilst maintaining deterministic forwarding for known infrastructure. Static routes offer predictability and operator control, dynamic routes enable automatic discovery during enrolment, and TTL-based expiration prevents stale route accumulation. The `RouteResolver` abstraction cleanly separates route management from forwarding logic, integrates with RIB for distributed state consistency, and provides optional TOML persistence for recovery after restarts. This approach aligns with RINA's distributed architecture principles whilst offering practical operational characteristics for real-world deployments.
