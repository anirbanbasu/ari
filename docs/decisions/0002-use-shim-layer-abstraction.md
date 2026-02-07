---
parent: Decisions
nav_order: 2
# These are optional metadata elements. Feel free to remove any of them.
status: "accepted"
date: 2026-01-31
decision-makers:
    - abasu
consulted:
    - nacarino
informed:
    - nacarino
---
# Use Shim layer abstraction for multiple underlay support

## Context and Problem Statement

RINA is recursive by definition. However, until we have a RINA-only network stack, we need to support multiple underlays (e.g., UDP, TCP, etc.) for the bottommost DIF to communicate between heterogeneous architectures. Also, this helps us focus on RINA-specific functionality and research explorations with RINA, without being restricted by the choice or the absence of an underlay protocol. To achieve this, we need to decide on an approach to abstract the underlay protocols.

**What is Shim?**: A Shim layer is an abstraction layer that sits between two other layers in a network stack. It provides a consistent interface for the upper layer to interact with the lower layer. The word "shim" originates from the physical world, where a shim is a thin piece of material used to fill gaps or spaces between objects to ensure proper alignment or fit. See the [Cambridge Dictionary entry for "shim"](https://dictionary.cambridge.org/dictionary/english/shim).

## Considered Options

* Single underlay support with UDP only.
* Multiple underlay support with dedicated implementations for each underlay.
* Shim layer abstraction with `Shim` trait to support multiple underlays, with a default implementation for UDP.

## Decision Outcome

Chosen option: "Shim layer abstraction with `Shim` trait to support multiple underlays, with a default implementation for UDP", because it provides the best balance between immediate functionality and future extensibility. This approach allows us to focus on RINA-specific research and functionality while maintaining flexibility to integrate different underlay protocols as needs evolve.

## Pros and Cons of the Options

### Single underlay support with UDP only

* Good, because it is simple to implement and test with minimal abstraction overhead.
* Good, because UDP provides connectionless, low-latency communication suitable for initial RINA experimentation.
* Bad, because it locks the implementation to a single transport protocol, limiting deployment scenarios.
* Bad, because switching to a different underlay later would require significant refactoring of core components.
* Bad, because it prevents exploration of RINA behaviour over different transport characteristics (e.g., reliability, ordering).

### Multiple underlay support with dedicated implementations for each underlay

* Good, because it allows protocol-specific optimisations for each underlay.
* Bad, because it leads to code duplication across different underlay implementations.
* Bad, because adding new underlay protocols requires modifying core RINA components (EFCP, RMT, enrollment).
* Bad, because it violates the open/closed principle - not extensible without modification.
* Bad, because testing and maintenance burden grows linearly with each new underlay protocol.

### Shim layer abstraction with `Shim` trait to support multiple underlays, with a default implementation for UDP

* Good, because it provides a clean separation of concerns between RINA logic and underlay transport.
* Good, because the `Shim` trait defines a minimal interface (`bind`, `send_pdu`, `receive_pdu`, `register_peer`, `lookup_peer`) that any transport can implement.
* Good, because new underlay protocols can be added without modifying existing RINA components, following the open/closed principle.
* Good, because it enables future support for TCP (reliable streams), QUIC (modern transport with built-in encryption), Unix sockets (local IPC), or even other RINA DIFs as underlay.
* Good, because UDP implementation (`UdpShim`) serves as a reference for future implementations.
* Good, because address mapping is encapsulated within the shim, automatically translating RINA addresses to socket addresses.
* Neutral, because it introduces one additional abstraction layer, though with minimal performance overhead.
* Bad, because the trait must balance generality (supporting diverse transports) with specificity (exposing necessary functionality).

## Conclusion

We choose the Shim layer abstraction with `Shim` trait to support multiple underlays, with a default UDP implementation (`UdpShim`), to enable flexibility and extensibility in the network stack while maintaining a consistent interface for different underlay protocols. This approach decouples RINA logic from transport details, allowing for easier integration of new underlay protocols (TCP, QUIC, Unix sockets, or even RINA DIFs as N-1 layers) in the future without significant changes to the core architecture. The trait-based design follows Rust best practices and enables compile-time polymorphism for efficient dispatch.
