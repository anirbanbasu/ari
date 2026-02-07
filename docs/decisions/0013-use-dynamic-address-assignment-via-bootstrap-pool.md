---
parent: Decisions
nav_order: 13
# These are optional metadata elements. Feel free to remove any of them.
status: "proposed"
date: 2026-02-07
decision-makers:
    - abasu
---
# Use dynamic address assignment via bootstrap allocation pool

## Context and Problem Statement

RINA Inter-Process Communication Processes (IPCPs) require unique addresses within a Distributed IPC Facility (DIF) to route Protocol Data Units correctly. Manually configuring addresses for each member IPCP becomes operationally burdensome in large DIFs (dozens to hundreds of members) and error-prone (duplicate addresses cause routing conflicts). Member IPCPs joining dynamically (e.g., container orchestration scaling, cloud auto-scaling) need addresses without administrator intervention. We need an address assignment mechanism that balances automation (no manual configuration), uniqueness guarantees (no conflicts), and simplicity (minimal coordination overhead).

## Considered Options

* Use static address assignment only.
* Use bootstrap IPCP maintains `AddressPool` and assigns addresses to member IPCPs during enrollment.
* Use Distributed Hash Table (DHT)-based allocation with peer coordination.
* Use centralized address server separate from bootstrap IPCP.

## Decision Outcome

Chosen option: "Use bootstrap IPCP maintains `AddressPool` and assigns addresses to member IPCPs during enrollment", because it integrates address assignment naturally into the existing enrollment workflow without requiring additional infrastructure or complex distributed coordination. The bootstrap IPCP maintains an `AddressPool` (configurable range, e.g., 1000-9999) and allocates addresses sequentially during enrollment when members request dynamic assignment (`request_address: true`). This approach supports both static configuration (members with pre-configured addresses) and dynamic allocation (members requesting addresses) within the same DIF, providing operational flexibility.

## Pros and Cons of the Options

### Use bootstrap IPCP maintains `AddressPool` and assigns addresses to member IPCPs during enrollment

* Good, because address assignment integrates seamlessly into enrollment—single request/response exchange, no additional round trips.
* Good, because bootstrap IPCP already acts as DIF coordinator (manages enrollment, RIB synchronization), centralizing address allocation is natural.
* Good, because `AddressPool` (range start/end, `HashSet<u64>` tracking assigned addresses) is simple—O(n) allocation scan, O(1) lookup/release.
* Good, because members request addresses via `EnrollmentRequest::request_address` flag (set when `ipcp_address == 0`), enabling mixed static/dynamic addressing.
* Good, because address exhaustion handled gracefully—bootstrap rejects enrollment with error message, allowing retry after releases.
* Good, because `AddressPool::release()` supports address reclamation when members leave (e.g., container shutdown, IPCP crash).
* Good, because allocation range (`pool_start`, `pool_end`) is configurable at bootstrap initialization, supporting different DIF sizes (small: 10-100, large: 1000-10000).
* Good, because assigned addresses automatically stored in member RIB (`/local/address`) during enrollment, persisting across restarts.
* Neutral, because sequential allocation (1000, 1001, 1002...) is predictable but sufficient for RINA DIFs (no security-through-obscurity requirement).
* Bad, because bootstrap IPCP is single point of failure for address allocation—if bootstrap offline, new members cannot join (mitigated by re-enrollment on bootstrap recovery).
* Bad, because `AddressPool` state is in-memory—bootstrap restart loses assignment tracking, potentially causing duplicate allocations (mitigated by members retaining assigned addresses in RIB).

### Use static address assignment only

* Good, because static configuration is simple—no allocation logic, no state management, no exhaustion handling.
* Good, because administrators have full control over address space layout (e.g., 1000-1999 for data plane, 2000-2999 for control plane).
* Good, because static addresses enable predictable routing table sizing and optimization opportunities.
* Neutral, because static configuration works well for fixed-size DIFs with stable membership (e.g., datacenter backbone routers).
* Bad, because manual address assignment scales poorly—O(n) administrator effort for n members, delays deployment.
* Bad, because duplicate address detection requires manual coordination (spreadsheets, configuration management systems), error-prone.
* Bad, because dynamic scaling (cloud auto-scaling, container orchestration) impossible—new instances need pre-allocated addresses.
* Bad, because address reclamation requires manual tracking of departed members and configuration updates.

### Use Distributed Hash Table (DHT)-based allocation with peer coordination

* Good, because decentralized allocation eliminates single point of failure—any member can allocate addresses via DHT queries.
* Good, because DHT provides distributed consensus on address ownership, preventing conflicts without central authority.
* Good, because DHT scales horizontally—allocation performance constant regardless of DIF size (O(log n) DHT lookups).
* Good, because DHT naturally supports address reclamation via key expiration (time-to-live) or explicit release operations.
* Neutral, because DHT implementation complexity (Kademlia, Chord) requires routing table maintenance, replication, fault tolerance mechanisms.
* Bad, because DHT introduces latency—address allocation requires multiple DHT hops (typically O(log n)), delaying enrollment.
* Bad, because DHT requires peer discovery bootstrap (well-known nodes or seed list) before address allocation can begin (circular dependency).
* Bad, because DHT state convergence during network partitions can cause temporary address conflicts (split-brain scenarios).
* Bad, because DHT adds external dependency (DHT library, protocol implementation) increasing codebase complexity and maintenance burden.

### Use centralized address server separate from bootstrap IPCP

* Good, because dedicated address server can optimize for high-throughput allocation (database-backed, transactional guarantees).
* Good, because address server can persist allocation state (PostgreSQL, SQLite), surviving restarts without losing tracking.
* Good, because address server can be replicated (primary/backup, multi-primary with consensus) for high availability.
* Good, because separation of concerns—bootstrap IPCP handles enrollment protocol, address server handles allocation.
* Neutral, because address server requires separate deployment, configuration, and monitoring (operational overhead).
* Bad, because two-phase enrollment required—member contacts address server, receives address, then enrolls with bootstrap using assigned address.
* Bad, because additional network round trips delay enrollment—address server query adds latency before enrollment begins.
* Bad, because address server becomes separate single point of failure—if unreachable, enrollment blocked despite bootstrap availability.
* Bad, because address server introduces external dependency (database, service discovery), complicating deployment (especially embedded/edge scenarios).

## More Information

### Current Implementation

`AddressPool` in [src/directory.rs](src/directory.rs) provides address allocation:

#### Structure

* **Range**: `start: u64`, `end: u64` (inclusive, e.g., 1000-9999 provides 9000 addresses).
* **Tracking**: `Arc<RwLock<HashSet<u64>>>` stores currently assigned addresses.
* **Methods**:
  * `allocate()`: Scans start→end for first unassigned address, inserts into set, returns address (or error if exhausted).
  * `release(address)`: Removes address from set, making it available for reallocation.
  * `is_allocated(address)`: Checks if address currently assigned.
  * `available_count()`: Returns `capacity() - allocated_count()`.

#### Enrollment Integration

In [src/enrollment.rs](src/enrollment.rs):

* **Bootstrap initialization**: `EnrollmentManager::new_bootstrap(rib, shim, local_addr, pool_start, pool_end)` creates `AddressPool`.
* **Member request**: `EnrollmentRequest::request_address` flag set when member has `ipcp_address == 0`.
* **Allocation**: Bootstrap checks flag, calls `address_pool.allocate()`, includes result in `EnrollmentResponse::assigned_address`.
* **Storage**: Member receives assigned address, stores in RIB (`/local/address`), updates `local_addr` field.
* **Mapping**: Bootstrap calls `shim.register_peer(new_addr, socket_addr)` to map assigned RINA address to UDP socket.

#### Allocation Strategy

* **Sequential scan**: `for addr in start..=end` finds first available address (simple, deterministic).
* **O(n) worst case**: If pool nearly full, allocation scans many addresses (acceptable for typical DIF sizes <10,000).
* **No fragmentation handling**: Released addresses immediately available for reallocation.

### Design Rationale

* **Bootstrap centralization**: Leverages existing bootstrap IPCP role as DIF coordinator, avoiding separate infrastructure.
* **Enrollment integration**: Single round trip assigns address and completes enrollment, minimizing latency.
* **Mixed addressing**: Supports both static (pre-configured) and dynamic (pool-allocated) addresses in same DIF, enabling hybrid deployments (static for core routers, dynamic for edge members).
* **Simplicity**: In-memory `HashSet` tracking requires no external dependencies (databases, distributed coordination), suitable for embedded and edge scenarios.

### Conclusion

We choose dynamic address assignment via bootstrap allocation pool to enable automatic, conflict-free address assignment during enrollment whilst maintaining operational simplicity. The `AddressPool` (configurable range with `HashSet` tracking) integrates naturally into the enrollment workflow, supporting both static and dynamic addressing within the same DIF. This approach balances automation (no manual configuration), reliability (bootstrap manages uniqueness), and simplicity (no external dependencies or complex coordination protocols) for RINA DIF address management.
