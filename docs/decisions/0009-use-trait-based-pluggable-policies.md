---
parent: Decisions
nav_order: 9
# These are optional metadata elements. Feel free to remove any of them.
status: "proposed"
date: 2026-02-07
decision-makers:
    - abasu
---
# Use trait-based pluggable policy abstraction for routing, scheduling, and Quality of Service

## Context and Problem Statement

RINA components require configurable algorithms for routing path computation, Protocol Data Unit (PDU) scheduling disciplines, and Quality of Service (QoS) management to support diverse network environments and performance requirements. These algorithms must be swappable without modifying core RINA components (Relaying and Multiplexing Task, Error and Flow Control Protocol, shim layers), enabling experimentation with different strategies, optimisation for specific workloads, and runtime selection based on deployment scenarios. We need a mechanism that provides algorithm flexibility whilst maintaining type safety, avoiding code duplication, and enabling compile-time verification of policy implementations.

## Considered Options

* Use trait-based pluggable policy abstraction with concrete implementations (e.g., `ShortestPathRouting`, `PriorityScheduling`, `SimpleQoSPolicy`).
* Hard-code algorithms directly into core components with conditional logic for variants.
* Use strategy pattern with enums to select amongst predefined algorithms.
* Implement a plugin system with dynamic loading of algorithm implementations.

## Decision Outcome

Chosen option: "Use trait-based pluggable policy abstraction with concrete implementations", because Rust's trait system provides zero-cost abstractions with compile-time dispatch, enabling algorithm flexibility without runtime overhead. Policy traits (`RoutingPolicy`, `SchedulingPolicy`, `QoSPolicy`) define interfaces for routing computation, PDU queueing, and QoS decisions, whilst concrete implementations (`ShortestPathRouting`, `FifoScheduling`, `PriorityScheduling`, `SimpleQoSPolicy`) provide specific algorithms. This design follows Rust's idiomatic approach to abstraction, ensures type safety through trait bounds (`Send + Sync` for concurrency), and allows adding new policies without changing existing components.

## Pros and Cons of the Options

### Use trait-based pluggable policy abstraction with concrete implementations

* Good, because Rust traits enable polymorphism with zero runtime overhead through static dispatch (monomorphisation).
* Good, because trait bounds (`Send + Sync`) ensure thread-safe policy implementations compatible with actor-based concurrency (ADR 0003).
* Good, because new policies can be added by implementing traits without modifying existing code (open/closed principle).
* Good, because trait methods provide clear, well-defined interfaces: `compute_next_hop`, `enqueue`, `dequeue`, `check_qos`, `should_drop`.
* Good, because policies are testable in isolation with unit tests verifying correctness of individual algorithms.
* Good, because components accept policies as generic parameters (`impl RoutingPolicy`) or trait objects (`Box<dyn RoutingPolicy>`), enabling runtime selection.
* Good, because trait-based design maps naturally to RINA's layered architecture where policies are orthogonal to protocol mechanisms.
* Good, because concrete implementations are self-contained: `ShortestPathRouting` encapsulates Dijkstra's algorithm state without exposing internals.
* Neutral, because trait objects (`Box<dyn Trait>`) incur virtual dispatch overhead if dynamic selection is needed, though this is negligible for policy-level decisions.
* Bad, because adding new trait methods requires updating all existing implementations (mitigated by providing default implementations where appropriate).

### Hard-code algorithms directly into core components with conditional logic for variants

* Good, because it is simple to implement initially without abstraction layers.
* Good, because it eliminates indirection—algorithms are directly embedded in components.
* Neutral, because conditional logic (`if routing_mode == ShortestPath`) is straightforward for small numbers of variants.
* Bad, because it violates separation of concerns—core components become responsible for algorithm implementation details.
* Bad, because adding new algorithms requires modifying and recompiling core components, breaking the open/closed principle.
* Bad, because conditional logic grows complex as more algorithms are added (nested `if`/`match` statements).
* Bad, because it creates tight coupling between components and specific algorithms, making code harder to maintain.
* Bad, because testing individual algorithms requires setting up full component context rather than testing policies in isolation.

### Use strategy pattern with enums to select amongst predefined algorithms

* Good, because enums provide type-safe selection of algorithms at compile time.
* Good, because `match` expressions on enums ensure exhaustive handling of all variants.
* Good, because it avoids trait objects and virtual dispatch, maintaining static dispatch performance.
* Neutral, because algorithms can be grouped within enum variants with associated data (e.g., `RoutingAlgorithm::ShortestPath(state)`).
* Bad, because adding new algorithms requires extending enums and updating all `match` expressions, violating the open/closed principle.
* Bad, because enum-based approach does not scale well—large numbers of algorithms lead to unwieldy enums.
* Bad, because it lacks the flexibility of trait-based abstraction: cannot easily compose policies or accept user-defined implementations.
* Bad, because testing requires matching on enum variants rather than directly testing algorithm implementations.

### Implement a plugin system with dynamic loading of algorithm implementations

* Good, because it enables adding algorithms without recompiling the application (load from shared libraries at runtime).
* Good, because it provides maximum flexibility—new plugins can be developed independently and deployed separately.
* Good, because plugin systems enable third-party algorithm contributions without modifying core code.
* Neutral, because plugins require defining stable Application Binary Interface (ABI) boundaries for policy interfaces.
* Bad, because dynamic loading introduces runtime overhead (symbol resolution, function pointers) compared to static dispatch.
* Bad, because it complicates deployment—plugins must be versioned, installed correctly, and matched with compatible ARI versions.
* Bad, because plugin systems are complex to implement: safe FFI (Foreign Function Interface), version compatibility, error handling for missing/invalid plugins.
* Bad, because type safety is weakened—plugins loaded at runtime cannot be verified at compile time.
* Bad, because it is overkill for current requirements where compile-time policy selection is sufficient.

## More Information

### Current Implementation

ARI implements three policy trait abstractions in [src/policies/](src/policies/):

#### Routing Policy Trait

* **Trait definition** in [src/policies/routing.rs](src/policies/routing.rs):
  * `compute_next_hop(&self, src: u64, dst: u64, topology: &NetworkTopology) -> Option<u64>`: Determines next-hop address for routing
  * `update(&mut self, topology: &NetworkTopology)`: Recomputes routing based on topology changes
  * `name(&self) -> &str`: Returns policy identifier for logging and diagnostics
  * Trait bounds: `Send + Sync` for concurrent access across actors

* **Concrete implementation**: `ShortestPathRouting`
  * Algorithm: Dijkstra's shortest path computation
  * State: `routing_table: HashMap<(u64, u64), u64>` mapping (source, destination) to next-hop
  * Topology updates: Recomputes shortest paths for all nodes when `update()` is called
  * Use case: Minimise hop count or link cost for packet forwarding

* **Topology representation**: `NetworkTopology` struct
  * Adjacency list: `HashMap<u64, Vec<(u64, u32)>>` (node → [(neighbour, cost)])
  * Methods: `add_link`, `get_neighbors` for topology construction and queries

#### Scheduling Policy Trait

* **Trait definition** in [src/policies/scheduling.rs](src/policies/scheduling.rs):
  * `enqueue(&mut self, pdu: Pdu) -> Result<(), String>`: Adds PDU to transmission queue
  * `dequeue(&mut self) -> Option<Pdu>`: Retrieves next PDU to transmit
  * `queue_length(&self) -> usize`: Returns number of queued PDUs
  * `name(&self) -> &str`: Returns policy identifier
  * Trait bounds: `Send + Sync` for concurrent queueing operations

* **Concrete implementations**:

  **`FifoScheduling` (First-In-First-Out)**:
  * State: `VecDeque<Pdu>` with configurable `max_size` (default 1000 PDUs)
  * Discipline: Dequeues PDUs in insertion order
  * Use case: Fair queueing without priority differentiation

  **`PriorityScheduling`**:
  * State: `Vec<VecDeque<Pdu>>` with separate queues per priority level (default 4 levels, 250 PDUs each)
  * Discipline: Serves highest-priority queue first (priority 0-255 mapped to queue indices)
  * Method: `priority_to_queue_index(priority: u8)` maps PDU priority to queue index
  * Use case: Preferential treatment for high-priority traffic (control messages, real-time flows)

#### QoS Policy Trait

* **Trait definition** in [src/policies/qos.rs](src/policies/qos.rs):
  * `check_qos(&self, pdu: &Pdu) -> bool`: Validates PDU against QoS constraints
  * `apply_qos(&self, pdu: &mut Pdu, qos: QoSParameters)`: Applies QoS parameters to PDU
  * `should_drop(&self, pdu: &Pdu, queue_length: usize) -> bool`: Determines if PDU should be dropped based on congestion
  * `name(&self) -> &str`: Returns policy identifier
  * Trait bounds: `Send + Sync` for concurrent QoS decisions

* **Concrete implementation**: `SimpleQoSPolicy`
  * State: `max_queue_length` threshold (default 1000)
  * Drop logic:
    * At 75% capacity: Drop PDUs with priority < 128 (low priority)
    * At 100% capacity: Drop all PDUs (congestion control)
  * Use case: Basic Active Queue Management (AQM) with priority-aware tail drop

### Design Rationale

#### Zero-Cost Abstraction

* **Static dispatch**: Traits compiled with monomorphisation generate specialised code per implementation, equivalent to hand-written functions.
* **No virtual dispatch overhead**: When policies are statically known at compile time (e.g., `ShortestPathRouting`), no vtable lookups occur.
* **Dynamic dispatch option**: Trait objects (`Box<dyn RoutingPolicy>`) enable runtime policy selection with minimal overhead (single indirect call).

#### Open/Closed Principle

* **Open for extension**: New policies implemented by adding new structs that implement traits—no changes to existing code.
* **Closed for modification**: Core components (RMT, EFCP) accept policies generically (`impl SchedulingPolicy`) without knowing concrete types.
* **Example**: Adding `WeightedFairQueueing` requires implementing `SchedulingPolicy` trait, not modifying scheduler call sites.

#### Type Safety and Concurrency

* **Trait bounds**: `Send + Sync` ensure policies are safe to share across actor tasks (tokio async tasks).
* **Compile-time verification**: Trait implementations are checked at compile time, preventing runtime errors from incomplete implementations.
* **Ownership semantics**: Policies can be owned (`Box<dyn Policy>`), borrowed (`&dyn Policy`), or referenced immutably (`Arc<dyn Policy>`) based on usage patterns.

### Usage Patterns

#### Routing Policy Integration

* **Use case**: RMT actor computes next-hop for forwarding decisions
* **Integration**: RMT holds `routing_policy: Box<dyn RoutingPolicy>` and calls `routing_policy.compute_next_hop(src, dst, &topology)`
* **Topology updates**: When new neighbours discovered or links change, call `routing_policy.update(&new_topology)` to recompute paths

#### Scheduling Policy Integration

* **Use case**: EFCP or RMT enqueues outgoing PDUs and dequeues for transmission
* **Integration**: Component holds `scheduler: Box<dyn SchedulingPolicy>` and calls `scheduler.enqueue(pdu)` and `scheduler.dequeue()`
* **Example**: `PriorityScheduling` ensures control PDUs (high priority) are sent before data PDUs (low priority)

#### QoS Policy Integration

* **Use case**: Shim layer or RMT applies QoS constraints and drops PDUs under congestion
* **Integration**: Component holds `qos_policy: Box<dyn QoSPolicy>` and calls `qos_policy.should_drop(&pdu, queue_len)` before enqueuing
* **Example**: During congestion, `SimpleQoSPolicy` drops low-priority PDUs to protect high-priority flows

### Testing Strategy

* **Unit tests**: Each policy implementation has isolated tests in respective source files:
  * `test_shortest_path_routing`: Verifies Dijkstra's correctness on sample topology
  * `test_fifo_scheduling`: Validates FIFO order preservation
  * `test_priority_scheduling`: Confirms high-priority PDUs dequeued first
  * `test_qos_should_drop`: Checks drop logic at various congestion levels

* **Integration tests**: Components tested with different policy implementations to verify correct behaviour under various configurations

### Alternatives for Specific Use Cases

#### When Dynamic Loading Might Be Considered (Future Work)

* **Use case**: Research deployment where operators experiment with custom routing algorithms without recompiling ARI
* **Approach**: Define stable C ABI for policy traits, load implementations from shared libraries (`.so`, `.dylib`, `.dll`)
* **Trade-off**: Gains runtime flexibility but loses compile-time type safety and incurs FFI overhead

#### When Hard-Coded Algorithms Might Be Acceptable

* **Use case**: Embedded deployment with strict resource constraints requiring minimal binary size
* **Approach**: Compile with single policy implementation, eliminate trait abstraction overhead
* **Trade-off**: Reduces flexibility and testability but produces smallest possible binary

### Tooling and Extensibility

#### Adding New Policies

1. **Create implementation**: Define new struct (e.g., `WeightedFairQueueing`) in appropriate module
2. **Implement trait**: Provide methods required by trait interface (`enqueue`, `dequeue`, etc.)
3. **Add tests**: Write unit tests verifying correctness of new algorithm
4. **Export**: Add to module re-exports in [src/policies/mod.rs](src/policies/mod.rs)
5. **Use**: Instantiate and pass to components accepting trait (e.g., `Box::new(WeightedFairQueueing::new())`)

#### Policy Selection Mechanisms

* **Compile-time**: Instantiate specific policy in `main.rs` or configuration code (`let policy = ShortestPathRouting::new();`)
* **Runtime (via configuration)**: Parse TOML `routing_policy = "shortest-path"` and instantiate corresponding implementation
* **Composition**: Policies can wrap other policies (decorator pattern) for layered behaviour (e.g., `LoggingPolicy` wrapping `ShortestPathRouting`)

### Migration and Evolution

#### Current Limitations

* **Single policy per component**: Components currently use one routing policy, one scheduling policy, etc., rather than multiple concurrent policies.
* **No policy chaining**: Policies do not compose—cannot easily combine `WeightedFairQueueing` with `PriorityScheduling`.
* **Static topology**: `NetworkTopology` must be updated explicitly—no automatic topology discovery integration.

#### Future Enhancements

* **Adaptive routing**: Implement policies that adjust based on network congestion or failure detection (`AdaptiveRouting` trait implementation).
* **Weighted Fair Queueing (WFQ)**: Add scheduling policy providing bandwidth fairness across flows.
* **Explicit Congestion Notification (ECN)**: Extend `QoSPolicy` to support ECN marking rather than dropping PDUs.
* **Policy composition**: Enable chaining policies (e.g., `CompositScheduling` applying multiple disciplines in sequence).
* **Configuration-driven selection**: Parse policy names from TOML and instantiate via factory pattern.

### Conclusion

We choose trait-based pluggable policy abstraction because it provides zero-cost, type-safe algorithm flexibility essential for RINA's diverse deployment scenarios. The three policy traits (`RoutingPolicy`, `SchedulingPolicy`, `QoSPolicy`) define clear interfaces for path computation, PDU queueing, and QoS management, whilst concrete implementations (`ShortestPathRouting`, `FifoScheduling`, `PriorityScheduling`, `SimpleQoSPolicy`) provide specific algorithms. This design follows Rust's idiomatic approach to abstraction, ensures thread-safe implementations via `Send + Sync` trait bounds compatible with actor-based concurrency (ADR 0003), and adheres to the open/closed principle by enabling new policies without modifying core components. The trait-based approach offers superior extensibility, testability, and maintainability compared to hard-coded algorithms, enum-based strategies, or complex plugin systems, making it the optimal choice for ARI's policy abstraction layer.
