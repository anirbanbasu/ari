---
parent: Decisions
nav_order: 15
# These are optional metadata elements. Feel free to remove any of them.
status: "proposed"
date: 2026-02-07
decision-makers:
    - abasu
---
# Use Inter-Inter-Process Communication Process (IPCP) Flow Allocator for N-1 abstraction layer

## Context and Problem Statement

RINA Inter-Process Communication Processes communicate with neighbouring IPCPs through N-1 flows (flows provided by the underlying Distributed IPC Facility layer or shim). The Relaying and Multiplexing Task (RMT) receives Protocol Data Units (PDUs) for forwarding to next-hop neighbours, whilst the Shim layer provides underlay transport (User Datagram Protocol (UDP), Transmission Control Protocol (TCP)). Directly coupling RMT to Shim creates tight coupling between routing decisions and transport implementation, complicating flow lifecycle management (connection establishment, failure detection, statistics tracking). We need an abstraction layer that manages per-neighbour flow state, tracks connectivity health, collects statistics, and provides a clean interface between routing logic (RMT) and transport operations (Shim).

## Considered Options

* Use direct RMT-to-Shim communication without abstraction layer.
* Use Inter-IPCP Flow Allocator (`InterIpcpFlowAllocator`) between RMT and Shim to manage per-neighbour flow state.
* Use full Error and Flow Control Protocol (EFCP) flows between IPCPs with explicit flow allocation handshakes.
* Use connection pooling at Shim layer with transport-specific session management.

## Decision Outcome

Chosen option: "Use Inter-IPCP Flow Allocator between RMT and Shim to manage per-neighbour flow state", because it provides a clean N-1 abstraction layer that encapsulates flow lifecycle management without imposing full EFCP overhead for inter-IPCP communication. The `InterIpcpFlowAllocator` maintains a `HashMap<u64, InterIpcpFlow>` mapping remote RINA addresses to flow objects, tracking state (`InterIpcpFlowState`: Active, Stale, Failed), statistics (`sent_pdus`, `received_pdus`, `send_errors`), and activity timestamps (`last_activity: Instant`). RMT calls `get_or_create_flow(remote_addr)` before forwarding, lazily establishing flows via Resource Information Base (RIB) route lookups. This approach separates concerns—RMT focuses on routing decisions, FAL manages connectivity, Shim handles transport—enabling independent evolution of each layer.

## Pros and Cons of the Options

### Use Inter-IPCP Flow Allocator between RMT and Shim to manage per-neighbour flow state

* Good, because it provides clean separation between routing (RMT), connectivity management (FAL), and transport (Shim).
* Good, because lazy flow creation (`get_or_create_flow()`) avoids pre-establishing connections to all possible neighbours.
* Good, because `InterIpcpFlowState` enum (Active, Stale, Failed) enables connection health monitoring integrated with ADR 0014 heartbeat mechanism.
* Good, because per-flow statistics (`sent_pdus`, `received_pdus`, `send_errors`) support operational monitoring and debugging.
* Good, because `last_activity: Instant` tracking enables stale flow detection and automatic cleanup via `cleanup_stale_flows()`.
* Good, because dynamic address updates (`update_peer_address()`) handle underlay address changes (Dynamic Host Configuration Protocol (DHCP) renewal, Network Address Translation (NAT) rebinding) without breaking flows.
* Good, because automatic peer creation on reception (`record_received_from()`) discovers neighbours without explicit configuration.
* Good, because RIB-based route lookup (`lookup_route()`) unifies dynamic and static route resolution for flow establishment.
* Good, because `Arc<Mutex<HashMap>>` provides thread-safe flow access with minimal contention (short critical sections for statistics updates).
* Neutral, because abstraction layer adds indirection (RMT → FAL → Shim vs. RMT → Shim), though overhead is negligible (HashMap lookup).
* Bad, because flow state is in-memory only—IPCP restart loses flow statistics (acceptable since neighbours re-establish connectivity automatically).
* Bad, because `Mutex` serializes flow access—under high concurrency (thousands of PDUs/sec to different neighbours), lock contention may become bottleneck (solvable via sharding or lock-free structures if needed).

### Use direct RMT-to-Shim communication without abstraction layer

* Good, because it eliminates abstraction overhead—RMT directly calls `shim.send_pdu(pdu)`.
* Good, because simple implementation—no additional data structures or lifecycle management.
* Good, because minimal memory footprint—no per-neighbour flow state tracking.
* Neutral, because suitable for simple scenarios with stable, well-known neighbours.
* Bad, because tight coupling between RMT and Shim complicates independent evolution—changing transport layer requires RMT modifications.
* Bad, because no per-neighbour statistics—operational monitoring must parse Shim-level metrics (source/destination addresses in PDUs).
* Bad, because no flow state tracking—failures detected only via failed sends, no proactive health monitoring.
* Bad, because address updates require manual RMT reconfiguration—DHCP renewal breaks connectivity until administrator intervenes.
* Bad, because no stale flow cleanup—inactive neighbours remain in routing tables indefinitely, wasting memory and confusing operators.

### Use full EFCP flows between IPCPs with explicit flow allocation handshakes

* Good, because EFCP provides full RINA flow semantics—explicit allocation request/response, flow identifier assignment, quality-of-service (QoS) negotiation.
* Good, because EFCP includes flow control (sliding windows), retransmission (sequence numbers, acknowledgements), and congestion control.
* Good, because explicit flow allocation enables admission control—bootstrap IPCP can reject flows when overloaded.
* Good, because EFCP state machines provide well-defined failure handling (timeout, connection refused, flow torn down).
* Neutral, because EFCP aligns with RINA architectural principles—N-1 flows are first-class entities with lifecycle management.
* Bad, because EFCP overhead is excessive for inter-IPCP control plane communication—enrollment, Common Distributed Application Protocol (CDAP) messages do not require retransmission or flow control (handled at higher layers).
* Bad, because explicit flow allocation handshakes add latency—three-way handshake (allocate request, allocate response, acknowledge) delays first PDU transmission.
* Bad, because EFCP state per flow consumes significant memory—sequence numbers, retransmission buffers, window state for every neighbour.
* Bad, because EFCP implementation complexity (state machines, timers, retransmission logic) increases codebase size and maintenance burden.
* Bad, because inter-IPCP flows are effectively permanent (neighbours remain connected while enrolled)—EFCP's dynamic allocation/deallocation capabilities underutilised.

### Use connection pooling at Shim layer with transport-specific session management

* Good, because connection pooling at Shim enables transport-specific optimisations—TCP connection reuse, QUIC stream multiplexing.
* Good, because Shim-level pooling works across all RMT usage without explicit flow tracking.
* Good, because transport sessions can persist across temporary failures, reducing reconnection overhead.
* Neutral, because pooling is transparent to RMT—routing logic unaffected by underlying session management.
* Bad, because mixing connection pooling with connectionless transports (UDP) creates inconsistency—UDP shim cannot pool "connections".
* Bad, because Shim-level pooling lacks RINA address awareness—pools by socket addresses, not semantic neighbour relationships.
* Bad, because no per-neighbour statistics at RINA layer—operators see TCP connection counts, not flow activity by RINA address.
* Bad, because pool eviction policies (least recently used, time-to-live) conflict with RINA routing—active routes may have evicted connections.
* Bad, because error handling complexity—transport session failures must propagate to RMT for route invalidation, requiring cross-layer signalling.

## More Information

### Current Implementation

`InterIpcpFlowAllocator` in [src/inter_ipcp_fal.rs](src/inter_ipcp_fal.rs) provides N-1 abstraction:

#### Flow Structure

* **InterIpcpFlow**:
  * `remote_addr: u64`: Remote RINA address (next-hop neighbour).
  * `socket_addr: SocketAddr`: Underlay transport address (UDP socket).
  * `state: InterIpcpFlowState`: Active, Stale (no recent activity), or Failed (send errors).
  * `last_activity: Instant`: Timestamp of last send/receive for staleness detection.
  * `sent_pdus`, `received_pdus`, `send_errors: u64`: Per-flow statistics.

#### Flow Allocator

* **Structure**: `flows: Arc<Mutex<HashMap<u64, InterIpcpFlow>>>`, `rib: Rib`, `shim: Arc<dyn Shim>`, `stale_timeout: Duration` (default 300s).
* **Lazy creation**: `get_or_create_flow(remote_addr)` checks existing flows, calls `lookup_route()` if needed (searches RIB `/routing/dynamic/*` and `/routing/static/*`), registers peer with Shim, creates flow.
* **Sending**: `send_pdu(next_hop, pdu)` updates `sent_pdus` statistics, calls `shim.send_pdu(pdu)`, records errors via `record_send_error()` (sets state to Failed).
* **Reception**: `record_received_from(remote_addr, socket_addr)` updates `received_pdus`, creates flow if first contact, updates socket address if changed.

#### Integration with RMT

In [src/actors.rs](src/actors.rs):

* **RmtActor**: `flow_allocator: Option<Arc<InterIpcpFlowAllocator>>` field.
* **Initialization**: `rmt_actor.set_flow_allocator(flow_allocator)` before starting.
* **Forwarding logic**: On receiving `RmtMessage::Forward { pdu, next_hop }`:
  1. Call `flow_allocator.get_or_create_flow(next_hop).await` (establishes connectivity).
  2. Call `flow_allocator.send_pdu(next_hop, &pdu)` (transmits PDU, updates statistics).
  3. Error handling: Log failures, potentially invalidate routes (future enhancement).

#### Lifecycle Management

* **Stale detection**: `is_stale(timeout)` checks if `last_activity.elapsed() > timeout`.
* **Cleanup**: `cleanup_stale_flows()` removes flows exceeding `stale_timeout` (300s default), returns count removed.
* **Explicit closure**: `close_flow(remote_addr)` manually removes flow (e.g., neighbour de-enrollment).
* **Statistics**: `get_flow_stats()` returns `Vec<(addr, state, sent, received)>`, `active_flow_count()` counts Active flows.

### Design Rationale

* **N-1 abstraction**: Separates RINA addressing (64-bit addresses, route lookups) from transport addressing (socket addresses, shim operations).
* **Lazy establishment**: Flows created on-demand when RMT needs connectivity, avoiding premature resource allocation.
* **Health tracking**: State enum (Active/Stale/Failed) integrates with ADR 0014 connection monitoring for proactive failure detection.
* **Statistics visibility**: Per-flow counters support operational dashboards ("sent 1234 PDUs to neighbour 1003") and debugging.
* **Dynamic addressing**: `update_peer_address()` handles underlay mobility (DHCP, NAT) transparently to RMT.
* **Minimal overhead**: HashMap lookup plus statistics update (~100ns) negligible compared to network I/O (~1-10ms).

### Conclusion

We choose Inter-IPCP Flow Allocator to provide a clean N-1 abstraction layer between RMT (routing logic) and Shim (transport operations). The `InterIpcpFlowAllocator` manages per-neighbour flow state (`InterIpcpFlow` with Active/Stale/Failed states, statistics, activity timestamps), enabling lazy flow creation, connection health monitoring, operational visibility, and dynamic address updates. This approach balances architectural clarity (separation of concerns), operational needs (statistics, health tracking), and implementation simplicity (minimal overhead, in-memory state) without imposing full EFCP complexity on inter-IPCP communication. The abstraction facilitates independent evolution of routing, connectivity, and transport layers whilst maintaining clean interfaces between components.
