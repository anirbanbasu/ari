---
parent: Decisions
nav_order: 5
# These are optional metadata elements. Feel free to remove any of them.
status: "superseded by ADR-0016"
date: 2026-02-07
decision-makers:
    - abasu
---
# Use bincode for network protocol serialisation

## Context and Problem Statement

RINA Inter-Process Communication (IPC) Processes exchange Protocol Data Units (PDUs), Common Distributed Application Protocol (CDAP) messages, enrolment requests, and Resource Information Base (RIB) synchronisation data over the network. These structures must be serialised into binary form for transmission and deserialised upon reception. The choice of serialisation format affects performance (encoding/decoding speed and message size), cross-language interoperability, schema evolution support, and implementation complexity. We need a serialisation mechanism that is efficient for network communication whilst integrating naturally with Rust's type system.

## Considered Options

* Use bincode for binary serialisation via Serde.
* Use Protocol Buffers (protobuf) with schema definitions and code generation.
* Use MessagePack for compact, self-describing binary serialisation.
* Use JSON for human-readable text-based serialisation.
* Use Cap'n Proto or FlatBuffers for zero-copy serialisation.

## Decision Outcome

Chosen option: "Use bincode for binary serialisation via Serde", because it provides the simplest integration with Rust's `serde` ecosystem, requires no external schema files or code generation, and offers compact binary encoding suitable for network transmission. This choice prioritises rapid development and Rust-native ergonomics whilst acknowledging that cross-language interoperability may require future consideration if non-Rust RINA implementations need to communicate with ARI.

## Pros and Cons of the Options

### Use bincode for binary serialisation via Serde

* Good, because it integrates seamlessly with Rust's `serde` derive macros, requiring only `#[derive(Serialize, Deserialize)]` annotations.
* Good, because it produces compact binary representations (smaller than JSON, comparable to MessagePack).
* Good, because encoding and decoding are fast, with minimal overhead beyond direct memory operations.
* Good, because it requires no external tooling, schema files, or code generation steps in the build process.
* Good, because the implementation is straightforward: `bincode::serialize()` and `bincode::deserialize()` with automatic type inference.
* Good, because it handles complex nested structures (enums, structs, vectors) automatically through Serde's trait system.
* Neutral, because it is Rust-specific, meaning other language implementations would need bincode libraries (available for some languages) or alternative protocols.
* Neutral, because the binary format is not self-describing, requiring both ends to agree on message structure at compile time.
* Bad, because it lacks formal schema evolution mechanisms, making protocol versioning manual (requires explicit version fields in messages).
* Bad, because cross-language interoperability is limited compared to protocol-agnostic formats like Protocol Buffers.

### Use Protocol Buffers (protobuf) with schema definitions and code generation

* Good, because it provides language-agnostic interoperability with official implementations in many languages.
* Good, because it includes formal schema evolution with backward/forward compatibility guarantees.
* Good, because `.proto` schema files serve as explicit protocol documentation.
* Good, because it produces compact binary encoding with efficient varint compression.
* Good, because tooling support (validation, linting, breaking change detection) is mature and widely adopted.
* Neutral, because message size is comparable to bincode for most structures.
* Bad, because it requires maintaining separate `.proto` schema files alongside Rust type definitions, increasing maintenance burden.
* Bad, because it needs code generation (via `prost` or `protobuf` crates) integrated into the build process.
* Bad, because generated Rust types are less idiomatic than hand-written structures (e.g., using `Option<Box<T>>` for optional fields).
* Bad, because it adds complexity for a single-language implementation where schema evolution is not yet a requirement.

### Use MessagePack for compact, self-describing binary serialisation

* Good, because it provides better cross-language support than bincode whilst maintaining Serde integration.
* Good, because the format is self-describing, allowing dynamic inspection of messages without schema knowledge.
* Good, because it offers compact encoding comparable to bincode.
* Good, because it integrates with Serde, similar to bincode, requiring minimal code changes.
* Neutral, because encoding/decoding may be slightly slower than bincode due to self-describing metadata overhead.
* Neutral, because the format is more complex than bincode's raw binary representation.
* Bad, because self-describing metadata increases message size compared to bincode (typically 10-20% larger).
* Bad, because it lacks formal schema definition and evolution mechanisms (similar limitation to bincode).

### Use JSON for human-readable text-based serialisation

* Good, because it is universally supported across all programming languages and platforms.
* Good, because messages are human-readable, aiding debugging and protocol inspection.
* Good, because it integrates trivially with Serde (`serde_json` crate).
* Good, because browser-based tools and command-line utilities can easily inspect traffic.
* Bad, because text encoding produces significantly larger messages (often 2-3× larger than binary formats).
* Bad, because parsing is slower than binary formats, consuming more CPU for encoding/decoding.
* Bad, because it lacks type safety (numbers can lose precision, no distinction between similar types).
* Bad, because it is inefficient for high-throughput or bandwidth-constrained scenarios.

### Use Cap'n Proto or FlatBuffers for zero-copy serialisation

* Good, because zero-copy deserialisation enables extremely fast message processing (no parsing step).
* Good, because it provides excellent performance for large messages with nested structures.
* Good, because it supports schema evolution with compatibility guarantees.
* Good, because memory layout is designed for direct access without intermediate allocations.
* Neutral, because it requires schema definitions and code generation (similar to Protocol Buffers).
* Bad, because Rust support is less mature compared to Protocol Buffers or native Serde formats.
* Bad, because the programming model is less ergonomic in Rust (requires working with generated accessor methods rather than native structs).
* Bad, because it introduces significant complexity for modest-sized messages where zero-copy benefits are negligible.
* Bad, because alignment requirements and platform-specific concerns make the format more complex to work with.

## More Information

### Current Implementation

The implementation uses bincode throughout the networking stack:

* **PDU serialisation**: `Pdu::serialize()` and `Pdu::deserialize()` in [pdu.rs](src/pdu.rs) use `bincode::serialize()` and `bincode::deserialize()` for all PDU types (data, acknowledgement, control, management).
* **CDAP messages**: `CdapMessage` serialisation in [enrollment.rs](src/enrollment.rs) uses bincode for enrolment requests, responses, and RIB synchronisation messages.
* **Enrolment protocol**: All enrolment-related structures (`EnrollmentRequest`, `EnrollmentResponse`, `DifConfiguration`) are serialised with bincode.
* **RIB synchronisation**: RIB snapshots and incremental change logs are serialised as `Vec<u8>` using bincode.
* **Actor communication**: When PDUs are received from the network (via UDP shim), they are deserialised using `bincode::deserialize::<Pdu>()`.

All serialisable types derive `Serialize` and `Deserialize` from `serde`, enabling automatic bincode support without manual implementation.

### Performance Characteristics

Bincode's performance is well-suited for ARI's current requirements:

* **Encoding speed**: Fast enough that serialisation overhead is negligible compared to network I/O and cryptographic operations.
* **Message size**: PDUs with small payloads (e.g., enrolment requests) are typically 100-500 bytes, whilst data PDUs vary with payload size.
* **Decoding speed**: Deserialisation is fast, allowing high packet rates on modern hardware.

For the experimental and educational nature of ARI, these characteristics are more than sufficient. Production deployments requiring extreme throughput might revisit this decision.

### Schema Evolution Considerations

Bincode does not provide built-in schema evolution. Protocol versioning must be handled manually:

* **Version fields**: Messages can include explicit version numbers (e.g., `RouteSnapshot` has a `version` field).
* **Optional fields**: Using `Option<T>` allows backward-compatible additions, though removal or type changes require careful coordination.
* **Migration strategy**: Breaking changes would require implementing version negotiation during enrolment or maintaining parallel protocol versions.

If cross-version compatibility becomes critical, migrating to Protocol Buffers would provide formal schema evolution mechanisms.

### Cross-Language Interoperability

Bincode libraries exist for some non-Rust languages, but support is limited:

* **JavaScript/TypeScript**: Community libraries available but not officially maintained.
* **Python**: Limited bincode support; would likely require custom deserialiser or protocol adapter.
* **C/C++**: No standard bincode library; would need manual implementation or protocol translation layer.

If interoperability with non-Rust RINA implementations becomes a requirement, the recommended migration path would be:

1. Define Protocol Buffer schemas for all wire-format messages.
2. Implement parallel serialisation support (bincode for Rust-to-Rust, protobuf for cross-language).
3. Gradually deprecate bincode in favour of protobuf once cross-language communication is validated.
4. Retain bincode internally for RIB persistence and local state snapshots where language-agnostic format is unnecessary.

### Conclusion

We choose bincode as the network serialisation format for ARI's initial implementation due to its simplicity, performance, and seamless Rust integration. This decision aligns with ARI's current status as a Rust-centric experimental platform where rapid development and type-safe ergonomics are prioritised over cross-language interoperability. The choice is explicitly marked as "proposed" rather than "accepted" to reflect its transitional nature—whilst bincode serves current needs well, future requirements for formal schema evolution or multi-language RINA implementations may warrant migration to Protocol Buffers. The implementation maintains clear serialisation boundaries (via `Pdu::serialize()` / `deserialize()` methods and CDAP message handling), making such a migration feasible without requiring extensive refactoring of higher-level components.
