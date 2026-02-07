---
parent: Decisions
nav_order: 4
# These are optional metadata elements. Feel free to remove any of them.
status: "accepted"
date: 2026-02-07
decision-makers:
    - abasu
---
# Use specific Common Distributed Application Protocol operation patterns for Resource Information Base management

## Context and Problem Statement

The Common Distributed Application Protocol (CDAP) specification defines six operations for distributed object management: `Create`, `Delete`, `Read`, `Write`, `Start`, and `Stop`. These operations enable Resource Information Base (RIB) synchronisation and distributed state management across Inter-Process Communication (IPC) Processes within a Distributed IPC Facility (DIF). However, the specification does not prescribe exactly when each operation should be used for specific RINA workflows such as enrolment, RIB synchronisation, and connection management. We need to establish clear patterns for which CDAP operations to use for each distributed coordination task to ensure consistency and adherence to RINA principles.

## Considered Options

* Use all six CDAP operations uniformly across all workflows.
* Use only `Create`, `Read`, `Write`, `Delete` for object manipulation, reserving `Start`/`Stop` for future use.
* Map CDAP operations to specific RINA workflows based on semantic meaning.
* Create custom protocol operations outside of CDAP for RINA-specific coordination.

## Decision Outcome

Chosen option: "Use only `Create`, `Read`, `Write`, `Delete` for object manipulation, reserving `Start`/`Stop` for future use", because these four operations are sufficient for current enrolment, RIB synchronisation, and distributed state management workflows. The `Start` and `Stop` operations are included in the protocol definition for RINA specification compliance but are explicitly marked as not-yet-implemented, to be used for long-running operations such as connection monitoring subscriptions or continuous synchronisation streams when needed.

## Pros and Cons of the Options

### Use all six CDAP operations uniformly across all workflows

* Good, because it fully exercises the CDAP specification from the start.
* Good, because it provides maximum semantic expressiveness for different operation types.
* Bad, because `Start` and `Stop` semantics (initiating/terminating long-running operations) are not currently needed for enrolment or one-shot RIB synchronisation.
* Bad, because it introduces unnecessary complexity by requiring implementations of operations without clear current use cases.
* Bad, because it may lead to inconsistent usage patterns if developers are uncertain when to use `Start` vs `Create` or `Stop` vs `Delete`.

### Use only `Create`, `Read`, `Write`, `Delete` for object manipulation, reserving `Start`/`Stop` for future use

* Good, because the four CRUD-style operations map naturally to RIB object lifecycle: creating objects during enrolment, reading for queries, writing for updates, and deleting for cleanup.
* Good, because current workflows (enrolment request/response, RIB synchronisation) are fundamentally object-oriented operations that fit the CRUD model.
* Good, because it reserves `Start`/`Stop` for genuinely long-running operations such as heartbeat monitoring, continuous RIB sync subscriptions, or flow allocation procedures.
* Good, because the implementation returns "Operation not yet implemented" for `Start`/`Stop`, clearly signalling future work without breaking RINA specification compliance.
* Good, because tests cover all four actively used operations, ensuring correct behaviour.
* Neutral, because developers must understand which workflows use which operations, though this becomes clear through code documentation.
* Bad, because it defers defining semantics for `Start`/`Stop` until concrete use cases emerge.

### Map CDAP operations to specific RINA workflows based on semantic meaning

* Good, because it provides explicit workflow-to-operation mappings (e.g., enrolment always uses `Create`, synchronisation uses `Read`).
* Good, because it can leverage `Start` for initiating enrolment and `Stop` for teardown.
* Neutral, because it requires defining precise mappings between RINA concepts and CDAP operations.
* Bad, because enrolment is conceptually creating a new relationship (fits `Create` better than `Start`).
* Bad, because it may conflate "starting an operation" with "creating an object", leading to semantic confusion.
* Bad, because RINA workflows may not always fit cleanly into the six-operation model, forcing awkward mappings.

### Create custom protocol operations outside of CDAP for RINA-specific coordination

* Good, because it allows protocol operations tailored precisely to RINA workflows without CDAP constraints.
* Bad, because it violates RINA specification adherence, which mandates CDAP for distributed object management.
* Bad, because it fragments the protocol surface, requiring separate implementations for CDAP-compliant RIB operations and custom coordination.
* Bad, because it reduces interoperability with other RINA implementations that expect standard CDAP.
* Bad, because it eliminates the benefits of standardised distributed object management semantics.

## Current Usage Patterns

The implementation establishes the following operation patterns:

### `Create` - Object Creation

* **Enrolment requests**: Members send `Create` messages to establish new enrolment relationships with bootstrap IPCPs
* **RIB object instantiation**: Creating new objects in the distributed RIB during synchronisation
* **Use case**: Any operation that introduces new state into the distributed system

### `Read` - Query and Synchronisation

* **RIB synchronisation requests**: Members send `Read` operations (via `CdapMessage::new_sync_request`) to query RIB state
* **Synchronisation responses**: Bootstrap IPCPs respond with `Read` operations containing incremental changes or full snapshots
* **Object queries**: Retrieving specific RIB object values
* **Use case**: Non-mutating queries and pulling state updates

### `Write` - State Updates

* **RIB object updates**: Modifying existing object values
* **Configuration changes**: Updating distributed configuration parameters
* **Use case**: Mutating existing state without creating or deleting objects

### `Delete` - Object Removal

* **RIB cleanup**: Removing stale or invalid objects from the distributed RIB
* **Neighbour disconnection**: Deleting neighbour state when connections are lost
* **Use case**: Explicitly removing objects from the distributed state

### `Start` and `Stop` - Reserved for Future Use

* Currently return "Operation not yet implemented" in `CdapSession::process_message`
* **Intended future use cases**:
  * Starting/stopping continuous RIB synchronisation subscriptions
  * Initiating/terminating heartbeat monitoring sessions
  * Beginning/ending flow allocation procedures
  * Activating/deactivating connection monitoring

## Conclusion

We choose to use only the four CRUD-style CDAP operations (`Create`, `Read`, `Write`, `Delete`) for current RIB manipulation and distributed coordination workflows. This decision aligns with the object-oriented nature of enrolment (creating relationships), synchronisation (reading state), updates (writing changes), and cleanup (deleting objects). The `Start` and `Stop` operations remain defined in `CdapOpCode` for RINA specification compliance but are explicitly unimplemented, reserved for future long-running operation management such as continuous synchronisation streams or connection monitoring subscriptions. This approach provides a clear, semantically meaningful protocol surface whilst maintaining flexibility for future enhancements when genuine lifecycle-oriented operations are required.
