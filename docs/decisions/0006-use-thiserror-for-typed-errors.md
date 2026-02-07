---
parent: Decisions
nav_order: 6
# These are optional metadata elements. Feel free to remove any of them.
status: "accepted"
date: 2026-02-07
decision-makers:
    - abasu
---
# Use thiserror for typed error handling

## Context and Problem Statement

RINA components require robust error handling to propagate failures through multiple architectural layers: from network I/O (Shim), through routing and multiplexing (Relaying and Multiplexing Task, RMT), flow control (Error and Flow Control Protocol, EFCP), distributed coordination (Common Distributed Application Protocol, CDAP), enrolment, and Resource Information Base (RIB) management. Early implementation used string-based errors (`Result<T, String>`), which lack structure, context preservation, and type safety. We need an error handling strategy that provides meaningful error information, enables type-safe error propagation, maintains error context across component boundaries, and integrates naturally with Rust's error handling ecosystem.

## Considered Options

* Continue using string-based errors (`Result<T, String>`).
* Use manual enum-based errors with hand-written `Display` and `Error` trait implementations.
* Use `thiserror` for deriving error types with structured variants.
* Use `anyhow` for dynamic error handling with context chaining.
* Use `eyre` for error reporting with enhanced diagnostics.

## Decision Outcome

Chosen option: "Use `thiserror` for deriving error types with structured variants", because it provides type-safe, component-specific error enums with automatic trait implementations whilst maintaining zero runtime overhead. The `#[derive(Error)]` macro generates idiomatic `Display` and `Error` trait implementations from declarative error definitions, enabling structured error variants with context fields, automatic error conversion via `#[from]`, and hierarchical error types that align with RINA's layered architecture.

## Pros and Cons of the Options

### Continue using string-based errors (`Result<T, String>`)

* Good, because it requires no additional dependencies or learning curve.
* Good, because error messages are immediately human-readable without formatting.
* Bad, because it provides no structure—impossible to pattern match on specific error conditions programmatically.
* Bad, because error context is lost when propagating through layers (no preservation of underlying cause).
* Bad, because it encourages string concatenation for context, leading to verbose and inconsistent error messages.
* Bad, because there is no type-level distinction between errors from different components, making error handling at call sites ambiguous.
* Bad, because it violates Rust best practices for error handling, particularly the expectation that errors implement `std::error::Error`.

### Use manual enum-based errors with hand-written trait implementations

* Good, because it provides full control over error structure and formatting.
* Good, because it enables pattern matching on specific error variants.
* Good, because it allows rich error context through enum variant fields.
* Neutral, because it requires implementing `Display`, `Error`, and optionally `From` traits manually for each error type.
* Bad, because it involves significant boilerplate code (each error variant needs a `Display` format string).
* Bad, because maintaining consistency across error messages becomes manual and error-prone.
* Bad, because adding new error variants requires updating multiple trait implementations.
* Bad, because it does not scale well when you have many error types (Enrolment, RIB, RMT, EFCP, Shim, CDAP).

### Use `thiserror` for deriving error types with structured variants

* Good, because the `#[derive(Error)]` macro automatically implements `Display` and `Error` traits from `#[error(...)]` attributes.
* Good, because it supports structured error variants with named fields, enabling rich context (e.g., `InvalidState { expected: String, actual: String }`).
* Good, because the `#[from]` attribute enables automatic error conversion, simplifying error propagation chains.
* Good, because it produces zero runtime overhead—all code generation happens at compile time.
* Good, because error messages are defined declaratively alongside variant definitions, improving maintainability.
* Good, because it naturally supports hierarchical errors: component-specific errors (`EnrollmentError`, `RibError`) can be wrapped in a top-level `AriError` enum.
* Good, because it integrates seamlessly with Rust's `?` operator for concise error propagation.
* Good, because error types remain serialisable (can derive `Clone`, `Debug`, `PartialEq`) for testing and debugging.
* Neutral, because it requires adding a single dependency (`thiserror = "2.0"`), though this is a widely-adopted, stable crate.
* Bad, because compile-time error messages for incorrect attribute usage can occasionally be cryptic.

### Use `anyhow` for dynamic error handling with context chaining

* Good, because it provides a unified `anyhow::Error` type that can wrap any error, simplifying function signatures.
* Good, because it supports `.context()` for adding layers of explanatory text to errors as they propagate.
* Good, because it is designed for application-level error handling where recovering from specific errors is rare.
* Neutral, because errors become opaque (cannot pattern match on specific variants), suitable for error reporting but not recovery.
* Bad, because it uses dynamic dispatch (trait objects) with a small runtime cost for error handling.
* Bad, because it discourages structured error types, making it harder to handle specific error conditions programmatically.
* Bad, because it is better suited for application code (like `main.rs`) than library code where callers may want to inspect error details.
* Bad, because component boundaries become less clear—all errors become `anyhow::Error`, losing type-level component information.

### Use `eyre` for error reporting with enhanced diagnostics

* Good, because it provides rich error reporting with colour-coded, pretty-printed diagnostics.
* Good, because it supports custom report handlers for tailoring error presentation.
* Good, because it maintains backtraces and contextual information through error chains.
* Neutral, because it is similar to `anyhow` but with more focus on end-user error reporting.
* Bad, because it introduces more complexity than `thiserror`, with larger dependency footprint.
* Bad, because enhanced diagnostics are primarily useful in end-user applications, not within library components.
* Bad, because it shares the same limitation as `anyhow`—dynamic error types that cannot be pattern matched.
* Bad, because it is optimised for application-level error reporting rather than library-level structured errors.

## More Information

### Implementation Structure

The error system is organised hierarchically in [error.rs](src/error.rs):

#### Top-Level Error Type

* **`AriError`**: Unified error type for the entire ARI library, wrapping component-specific errors.
  * Uses `#[from]` conversions to automatically convert from component errors.
  * Enables `?` operator to propagate errors from any component into `AriError`.
  * Provides variants for configuration errors, network errors, timeouts, and unimplemented features.

#### Component-Specific Error Types

Each RINA component has a dedicated error enum with variants representing failure modes:

* **`EnrollmentError`**: Covers enrolment lifecycle failures (not enrolled, already enrolled, timeout, serialisation failures, connection lost, re-enrolment required).
* **`RibError`**: Handles RIB operations (object not found, already exists, invalid names/classes, serialisation failures, access denied).
* **`RmtError`**: Addresses routing and multiplexing issues (no route, route not found, queue full, forwarding failures).
* **`EfcpError`**: Manages flow control problems (flow not found, allocation failures, invalid configuration, sequence errors).
* **`ShimError`**: Captures underlay transport issues (bind failures, send/receive failures, socket closure, I/O errors).
* **`CdapError`**: Covers distributed protocol errors (invalid operation codes, message format errors, invoke ID mismatches).
* **`SerializationError`**: Handles data encoding/decoding issues (wraps `bincode::Error` and `serde_json::Error`).

#### Error Attributes

The implementation leverages `thiserror` attributes for declarative error definitions:

* **`#[error("...")]`**: Defines the display format string for each variant, supporting interpolation of variant fields.
* **`#[from]`**: Automatically derives `From<SourceError>` for error conversion, enabling seamless error propagation.
* **Named fields**: Error variants include context fields (e.g., `InvalidState { expected: String, actual: String }`), providing structured information beyond simple messages.

### Migration from String-Based Errors

The implementation maintains backward compatibility during the transition:

* **Conversion from String**: `impl From<String> for AriError` and `impl From<&str> for AriError` allow gradual migration of string-based errors.
* **Conversion to String**: `impl From<ComponentError> for String` enables older code expecting string errors to continue functioning.
* **Coexistence**: Some functions still return `Result<T, String>` (e.g., in [actors.rs](src/actors.rs), [inter_ipcp_fal.rs](src/inter_ipcp_fal.rs)), marking areas for future migration to typed errors.

This dual approach allows incremental refactoring without requiring a big-bang rewrite.

### Benefits Realised

Typed errors provide tangible improvements across the codebase:

* **Debuggability**: Structured error variants with named fields make it clear exactly what went wrong (e.g., `Timeout { attempts: 3 }` vs generic "timeout occurred").
* **Error recovery**: Calling code can match on specific error variants to implement recovery strategies (e.g., retry on `ConnectionLost`, abort on `AlreadyEnrolled`).
* **Testing**: Error conditions can be constructed explicitly and asserted precisely in tests.
* **Documentation**: Error types serve as API documentation, showing callers what failures to expect from each operation.
* **Compiler assistance**: Type checking ensures errors are handled or explicitly ignored, reducing forgotten error paths.

### Future Enhancements

Potential improvements to the error system:

* **Complete migration**: Convert remaining `Result<T, String>` functions to use typed errors, eliminating string-based errors entirely except at application boundaries.
* **Error codes**: Add numeric error codes for protocol-level error signalling (useful for serialising errors in CDAP responses).
* **Contextual wrappers**: Consider adding `.context()` methods (similar to `anyhow`) for component-specific errors whilst retaining structured variants.
* **Backtrace support**: Enable optional backtrace capture for debugging (requires nightly Rust or `backtrace` crate).

### Conclusion

We choose `thiserror` for error handling in ARI, establishing a hierarchical system of typed error enums that map to RINA's architectural components. Each error type (`EnrollmentError`, `RibError`, `RmtError`, `EfcpError`, `ShimError`, `CdapError`) provides structured variants with contextual fields, automatic `Display` and `Error` trait implementations, and seamless conversion to the unified `AriError` type. This approach eliminates the ambiguity and lost context of string-based errors whilst maintaining zero runtime overhead and idiomatic Rust error handling. The implementation supports incremental migration from legacy string errors through backward compatibility conversions, allowing the codebase to evolve gradually towards fully typed error handling. The decision is marked "accepted" as the typed error system is implemented and actively used throughout the codebase, with string-based errors remaining only in areas awaiting migration.
