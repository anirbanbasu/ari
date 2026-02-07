---
parent: Decisions
nav_order: 3
# These are optional metadata elements. Feel free to remove any of them.
status: "proposed"
date: 2026-02-06
decision-makers:
    - abasu
---
# Use Actor-based hybrid concurrency model for RINA components

## Context and Problem Statement

RINA components such as Error and Flow Control Protocol (EFCP), Relaying and Multiplexing Task (RMT), Resource Information Base (RIB), and Enrolment Manager require concurrent processing to handle multiple connections, data flows, and events efficiently. These components must operate independently while coordinating through message passing, necessitating a concurrency model that avoids shared mutable state and potential race conditions. Choosing an appropriate concurrency model is crucial for ensuring scalability, maintainability, and performance of the RINA implementation.

## Considered Options

* Thread-based concurrency model using OS threads.
* Pure async/await model using Rust's async ecosystem (tokio) without actors.
* Actor-based concurrency model using async/await with message-passing channels.
* Hybrid approach: Actor pattern implemented with tokio channels and async tasks.

## Decision Outcome

Chosen option: "Hybrid approach: Actor pattern implemented with tokio channels and async tasks", because it combines the architectural benefits of the actor model (encapsulated state, message-passing) with the efficiency of async/await for I/O-bound operations. The implementation uses tokio's `mpsc` channels for message passing between actors, with each actor running as an async task. This provides clean separation of concerns whilst avoiding the overhead of OS threads and the complexity of managing shared mutable state.

## Pros and Cons of the Options

### Thread-based concurrency model using OS threads

* Good, because it is straightforward to implement using Rust's standard library (`std::thread`, `std::sync::mpsc`).
* Good, because it provides true parallelism for CPU-bound tasks.
* Bad, because it can lead to high resource consumption due to the overhead of OS threads (typically 1-2MB stack per thread).
* Bad, because it may lead to complex synchronisation issues and potential deadlocks when sharing state.
* Bad, because it does not scale well with a large number of concurrent tasks (hundreds of components would require hundreds of threads).
* Bad, because context switching between OS threads can be expensive for I/O-bound operations like network communication.

### Pure async/await model using Rust's async ecosystem (tokio) without actors

* Good, because it allows for efficient handling of I/O-bound tasks with minimal resource consumption (tasks are lightweight, ~64 bytes).
* Good, because it leverages Rust's powerful async/await syntax for writing non-blocking code.
* Good, because tokio provides excellent networking primitives for UDP/TCP operations.
* Bad, because without the actor pattern, components would need to share state using `Arc<RwLock<T>>` or similar primitives, increasing complexity.
* Bad, because directly sharing mutable state between async tasks can lead to subtle concurrency bugs.
* Bad, because it lacks the clear message-passing API that the actor model provides, making component boundaries less explicit.
* Bad, because it may not be the best fit for CPU-bound tasks that require parallel processing.

### Actor-based concurrency model using async/await with message-passing channels

* Good, because it encapsulates state and behaviour within actors, eliminating shared mutable state and associated race conditions.
* Good, because each actor (RibActor, EfcpActor, RmtActor, ShimActor) has a clear, well-defined message interface (`enum` types for message variants).
* Good, because it allows for easy scaling by distributing actors across multiple async tasks.
* Good, because it aligns perfectly with the message-passing paradigm of RINA (CDAP messages, PDU forwarding), facilitating natural communication patterns between components.
* Good, because typed message enums provide compile-time guarantees about valid operations on each component.
* Good, because actor handles (`ActorHandle<T>`) provide a clean, type-safe API for inter-component communication.
* Neutral, because message passing introduces minor overhead (channel send/receive), though this is negligible compared to network I/O.
* Neutral, because it requires using `tokio::spawn` to run actors as independent tasks.
* Bad, because it may require an additional learning curve for developers unfamiliar with the actor model.

### Hybrid approach: Actor pattern implemented with tokio channels and async tasks

* Good, because it combines the best aspects of both async/await and actor patterns.
* Good, because tokio's `mpsc` channels provide efficient, zero-allocation message passing for most cases.
* Good, because actors run as lightweight async tasks rather than OS threads, enabling thousands of concurrent actors.
* Good, because each actor encapsulates its state (wrapped in `Arc<RwLock<T>>` internally) with external access only via typed messages.
* Good, because the implementation is idiomatic Rust using standard tokio primitives (`tokio::sync::mpsc`, `tokio::spawn`).
* Good, because response channels in messages enable request-reply patterns without breaking actor encapsulation.
* Good, because the approach naturally handles backpressure through bounded channels.
* Good, because actors can use async I/O operations (network, file) whilst maintaining the actor pattern's benefits.
* Neutral, because actors internally use `Arc<RwLock<T>>` for state, but this is hidden from external callers.
* Bad, because it requires understanding both async/await and the actor pattern, though each individually is well-documented.

## Conclusion

We choose the hybrid approach of implementing the actor pattern using tokio channels and async tasks for RINA components. This encapsulates state and behaviour within actors (`RibActor`, `EfcpActor`, `RmtActor`, `ShimActor`), eliminating shared state issues whilst leveraging the efficiency of async I/O for network operations. Each actor exposes a typed message interface (`RibMessage`, `EfcpMessage`, etc.) that enforces valid operations at compile time. The implementation uses tokio's `mpsc::channel` for message passing and `tokio::spawn` for running actors as independent tasks. Whilst this requires understanding both async/await and actor patterns, the benefits in terms of scalability, maintainability, type safety, and natural alignment with RINA's message-passing architecture make it the most suitable choice. The approach also enables future enhancements such as distributed actors or actor supervision hierarchies if needed.
