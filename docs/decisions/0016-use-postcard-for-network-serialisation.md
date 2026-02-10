---
parent: Decisions
nav_order: 16
# These are optional metadata elements. Feel free to remove any of them.
status: "proposed"
date: 2026-02-10
decision-makers:
    - abasu
---
# Use postcard for network protocol serialisation

## Context and Problem Statement

ADR-0005 established bincode as the network serialisation format for RINA Inter-Process Communication (IPC) Processes, enabling the exchange of Protocol Data Units (PDUs), Common Distributed Application Protocol (CDAP) messages, enrolment requests, and Resource Information Base (RIB) synchronisation data. However, as stated on [crates.io](https://crates.io/crates/bincode), bincode development has ceased: "Due to a doxxing and harassment incident, development on bincode has ceased. No further releases will be published on crates.io." This creates a long-term maintenance risk and necessitates migration to an actively maintained alternative. We need a serialisation format that maintains bincode's performance and simplicity whilst providing active maintenance and future development.

## Considered Options

* Use postcard for compact binary serialisation via Serde.
* Use rkyv for zero-copy serialisation with archived types.
* Use rmp-serde (MessagePack) for compact, self-describing binary serialisation.
* Use ciborium (CBOR) for standards-compliant binary serialisation.
* Use borsh for deterministic binary serialisation.
* Use Protocol Buffers (protobuf) with schema definitions and code generation.

## Decision Outcome

Chosen option: "Use postcard for compact binary serialisation via Serde", because it provides the most straightforward migration path from bincode whilst offering comparable performance, active maintenance, and `no_std` compatibility for potential embedded use cases. Postcard was explicitly designed as a bincode successor with similar design philosophy (compact, fast, Serde-based) and minimal API surface, making the migration from bincode straightforward with limited code changes.

## Pros and Cons of the Options

### Use postcard for compact binary serialisation via Serde

* Good, because it integrates seamlessly with Rust's `serde` ecosystem, requiring only minor API changes from bincode.
* Good, because it produces extremely compact binary representations, often smaller than bincode due to varint encoding.
* Good, because it is actively maintained with a clear development roadmap and responsive maintainer.
* Good, because it supports `no_std` environments, enabling potential future use in embedded or resource-constrained scenarios.
* Good, because the migration from bincode is minimal: `postcard::to_allocvec()` and `postcard::from_bytes()` have similar ergonomics to bincode's API.
* Good, because it includes built-in flavors for customising serialisation behaviour (e.g., COBS encoding for framing).
* Good, because encoding and decoding are fast, with performance comparable to or better than bincode for most use cases.
* Good, because it has comprehensive documentation and active community support.
* Neutral, because it is Rust-specific (like bincode), requiring postcard libraries for other language implementations.
* Neutral, because the binary format is not self-describing, requiring both ends to agree on message structure at compile time.
* Bad, because it lacks formal schema evolution mechanisms (similar to bincode), making protocol versioning manual.
* Bad, because cross-language interoperability is limited compared to protocol-agnostic formats like Protocol Buffers.

### Use rkyv for zero-copy serialisation with archived types

* Good, because zero-copy deserialisation provides exceptional performance—data can be accessed directly without deserialisation overhead.
* Good, because it is actively maintained with strong performance focus and growing adoption.
* Good, because it supports validation to ensure archived data integrity.
* Good, because memory layout is designed for direct access, enabling extremely fast message processing for large structures.
* Good, because it supports schema evolution through careful type design and versioning.
* Good, because it can provide significant performance benefits for high-throughput scenarios.
* Neutral, because it works with Serde through `rkyv_derive` but also offers native derive macros.
* Bad, because it requires working with archived types (`ArchivedPdu`, `ArchivedCdapMessage`) rather than original types, increasing cognitive overhead.
* Bad, because the migration from bincode is more invasive, requiring code changes throughout the networking stack to handle archived representations.
* Bad, because the API is more complex than Serde-based approaches, with additional concepts like `Archive`, `Serialize`, `Deserialize` traits (distinct from Serde's).
* Bad, because the programming model requires understanding memory layout, alignment, and pinning concerns.
* Bad, because for modest-sized messages (typical in RINA PDUs), the zero-copy benefits may not outweigh the increased complexity.
* Bad, because debugging is more difficult when working with archived representations rather than native types.

### Use rmp-serde (MessagePack) for compact, self-describing binary serialisation

* Good, because it provides excellent cross-language support with MessagePack implementations in virtually all languages.
* Good, because the format is self-describing, allowing dynamic inspection of messages without schema knowledge.
* Good, because it integrates seamlessly with Serde, requiring minimal migration effort from bincode.
* Good, because it is actively maintained with mature ecosystem support.
* Good, because it offers compact encoding comparable to postcard and bincode.
* Neutral, because encoding/decoding may be slightly slower than postcard or bincode due to self-describing metadata overhead.
* Neutral, because message size is typically 10-20% larger than postcard due to type tags and metadata.
* Bad, because the self-describing format increases complexity for protocol implementations where schema is known at compile time.
* Bad, because it lacks formal schema definition and evolution mechanisms (though the format itself is more flexible than bincode).

### Use ciborium (CBOR) for standards-compliant binary serialisation

* Good, because CBOR (RFC 8949) is an IETF standard with formal specification and wide industry adoption.
* Good, because it provides excellent cross-language support with standardised implementations.
* Good, because it integrates with Serde, requiring minimal migration effort from bincode.
* Good, because it is actively maintained with strong standards compliance focus.
* Good, because the format supports extensibility and schema evolution through CBOR tags.
* Good, because it includes built-in support for additional data types (e.g., timestamps, UUIDs) beyond basic Serde types.
* Neutral, because encoding/decoding performance is comparable to MessagePack but slightly slower than postcard.
* Neutral, because message size is similar to MessagePack (larger than postcard but smaller than JSON).
* Bad, because the standards-compliance overhead may be unnecessary for a Rust-specific implementation.
* Bad, because CBOR's rich type system introduces complexity that is not required for RINA's current needs.

### Use borsh for deterministic binary serialisation

* Good, because it provides deterministic serialisation (same input always produces identical output), useful for hashing and consensus.
* Good, because it is actively maintained, primarily by the NEAR Protocol blockchain project.
* Good, because it integrates with Serde and offers simple, efficient binary encoding.
* Good, because it has strong focus on strict specification and determinism.
* Neutral, because it is primarily used in blockchain contexts, with less general-purpose adoption than postcard or MessagePack.
* Neutral, because performance is comparable to postcard but optimised for different use cases (determinism over size).
* Bad, because the determinism guarantees, whilst valuable for blockchain, are not required for RINA network protocols.
* Bad, because the ecosystem is smaller than postcard, with less general-purpose documentation and examples.

### Use Protocol Buffers (protobuf) with schema definitions and code generation

* Good, because it provides language-agnostic interoperability with official implementations in many languages.
* Good, because it includes formal schema evolution with backward/forward compatibility guarantees.
* Good, because `.proto` schema files serve as explicit protocol documentation.
* Good, because tooling support (validation, linting, breaking change detection) is mature and widely adopted.
* Neutral, because message size is comparable to postcard for most structures.
* Bad, because it requires maintaining separate `.proto` schema files alongside Rust type definitions, significantly increasing maintenance burden.
* Bad, because it needs code generation (via `prost` or `protobuf` crates) integrated into the build process.
* Bad, because generated Rust types are less idiomatic than hand-written structures.
* Bad, because it adds substantial complexity for a single-language implementation where schema evolution is not yet a requirement.
* Bad, because migration from bincode would be extensive, requiring schema definition for all wire-format types.

## More Information

### Migration Path from Bincode

The migration from bincode to postcard is straightforward due to API similarities:

**Before (bincode):**

```rust
// Serialisation
let bytes = bincode::serialize(&pdu)?;

// Deserialisation
let pdu: Pdu = bincode::deserialize(&bytes)?;
```

**After (postcard):**

```rust
// Serialisation
let bytes = postcard::to_allocvec(&pdu)?;

// Deserialisation
let pdu: Pdu = postcard::from_bytes(&bytes)?;
```

All existing `#[derive(Serialize, Deserialize)]` annotations remain unchanged. The primary changes are:

1. Replace `bincode::serialize()` with `postcard::to_allocvec()` (or `to_stdvec()` for `std` environments).
2. Replace `bincode::deserialize()` with `postcard::from_bytes()`.
3. Update `Cargo.toml` to replace `bincode = "1.3"` with `postcard = { version = "1.0", features = ["alloc"] }`.
4. Update error handling to use `postcard::Error` instead of `bincode::Error`.

### Performance Characteristics

Postcard's performance is well-suited for ARI's requirements:

* **Encoding speed**: Comparable to or faster than bincode for most structures due to optimised varint encoding.
* **Message size**: Often smaller than bincode due to variable-length integer encoding (varints) for lengths and numeric values.
* **Decoding speed**: Fast deserialisation with minimal overhead, comparable to bincode.
* **Memory efficiency**: Lower memory allocation overhead than bincode in many cases.

For ARI's current experimental and educational use case, postcard's performance characteristics exceed requirements whilst maintaining simplicity.

### Comparison with rkyv

Whilst rkyv offers superior raw performance through zero-copy deserialisation, the trade-offs make it less suitable for ARI's current needs:

**When rkyv excels:**

* High-throughput scenarios processing thousands of large messages per second.
* Applications where deserialisation overhead is a measured bottleneck.
* Use cases requiring direct memory-mapped access to serialised data.

**Why postcard is preferred for ARI:**

* RINA PDUs are typically small (100-500 bytes for control messages, variable for data PDUs).
* Deserialisation overhead is negligible compared to network I/O and cryptographic operations.
* Working with native types (`Pdu`, `CdapMessage`) is more intuitive than archived types.
* The migration from bincode is simpler, requiring minimal code changes.
* Zero-copy benefits are minimal for the message sizes and throughput ARI currently handles.

If future profiling identifies serialisation as a performance bottleneck, migration to rkyv could be reconsidered. However, for the current implementation, postcard's simplicity and performance balance is optimal.

### Schema Evolution and Versioning

Like bincode, postcard does not provide built-in schema evolution. Protocol versioning must be handled manually:

* **Version fields**: Messages should include explicit version numbers (e.g., `protocol_version: u16`).
* **Optional fields**: Using `Option<T>` allows backward-compatible additions.
* **Enums for variants**: Enum-based message types enable protocol extensions without breaking existing implementations.

Postcard's smaller message sizes (due to varint encoding) actually provide slight headroom for adding version metadata without increasing total message size beyond bincode's baseline.

If formal schema evolution becomes critical, the migration path remains the same as outlined in ADR-0005: transition to Protocol Buffers for wire-format messages whilst potentially retaining postcard for internal RIB persistence.

### No_std Compatibility

Postcard's `no_std` support provides future flexibility:

* Enables potential deployment in embedded systems or resource-constrained environments.
* Allows ARI components to run in kernel-space or bare-metal contexts if needed.
* Supports use cases where the standard library is unavailable or undesirable.

This is not an immediate requirement for ARI but provides valuable optionality for future experimental deployments.

### Cross-Language Interoperability

Postcard libraries exist for other languages, though support is less mature than MessagePack or Protocol Buffers:

* **C**: Community implementations available.
* **Python**: Limited support; would likely require custom implementation or bindings.
* **JavaScript/TypeScript**: Nascent support; not production-ready.

If cross-language RINA implementations become a priority, the recommended strategy is:

1. Implement Protocol Buffer schemas for all wire-format messages.
2. Support dual serialisation: postcard for Rust-to-Rust communication, protobuf for cross-language scenarios.
3. Use content negotiation during enrolment to determine which format to use.
4. Retain postcard for internal operations (RIB persistence, local caching) where cross-language support is unnecessary.

This approach balances ARI's current Rust-centric development with potential future interoperability needs.

### Relationship to ADR-0005

This ADR supersedes ADR-0005's choice of bincode whilst maintaining the same decision rationale: prioritising simplicity, performance, and Rust-native ergonomics for an experimental platform. The change is driven solely by bincode's maintenance status, not by fundamental dissatisfaction with the original decision. Postcard represents the natural evolution of ADR-0005's principles given the changed ecosystem landscape.

### Conclusion

We propose migrating from bincode to postcard for ARI's network serialisation layer. This change is necessitated by bincode's ceased development but represents an opportunity to adopt an actively maintained library with comparable (or superior) characteristics. Postcard's design as a bincode successor, combined with its `no_std` support and compact encoding, makes it the optimal choice for ARI's current needs. The migration is straightforward, requiring minimal code changes and no architectural restructuring. Alternative approaches (rkyv, MessagePack, CBOR) remain viable options if future requirements shift towards maximum performance (rkyv) or cross-language interoperability (MessagePack/CBOR), but postcard provides the best balance of simplicity, performance, and maintainability for ARI's present experimental and educational goals.
