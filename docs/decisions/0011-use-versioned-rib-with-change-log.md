---
parent: Decisions
nav_order: 11
# These are optional metadata elements. Feel free to remove any of them.
status: "proposed"
date: 2026-02-07
decision-makers:
    - abasu
---
# Use versioned Resource Information Base with change log for incremental synchronisation

## Context and Problem Statement

RINA Inter-Process Communication Processes (IPCPs) within a Distributed IPC Facility must synchronise their Resource Information Base (RIB) state to maintain consistency across distributed members. Full RIB snapshots can be large (thousands of objects including routes, flows, neighbour information, policies), making complete transfers inefficient for frequent synchronisation. Members that briefly disconnect or experience network delays should be able to catch up without retrieving the entire RIB. We need a mechanism to track RIB changes and enable incremental synchronisation whilst handling cases where change history is insufficient (requiring fallback to full snapshots).

## Considered Options

* Use versioned RIB with change log (`VecDeque<RibChange>`) for incremental synchronisation via Common Distributed Application Protocol (CDAP).
* Always use full RIB snapshots for every synchronisation request.
* Use database-backed RIB with transaction log and SQL-based delta queries.
* Use event sourcing with complete change history persisted to disk.

## Decision Outcome

Chosen option: "Use versioned RIB with change log for incremental synchronisation via CDAP", because it provides efficient delta-based synchronisation for common scenarios (brief disconnections, periodic updates) whilst maintaining simplicity through in-memory bounded change logs. Each RIB object has a monotonically increasing version number, and a `RibChangeLog` (bounded `VecDeque<RibChange>` with configurable size, default 1000 entries) tracks recent Creates, Updates, and Deletes. Members request changes since their last known version via CDAP Read operations, receiving either incremental deltas (if within change log retention) or full snapshots (if version too old). This approach balances efficiency, memory usage, and implementation complexity without requiring external database dependencies.

## Pros and Cons of the Options

### Use versioned RIB with change log for incremental synchronisation via CDAP

* Good, because incremental synchronisation reduces network bandwidth and processing time for members with recent state.
* Good, because version numbers (`u64` counters) provide total ordering of changes for consistency verification.
* Good, because bounded `VecDeque<RibChange>` limits memory usage (configurable max size, default 1000 changes).
* Good, because `RibChangeLog::get_changes_since(version)` efficiently retrieves deltas for CDAP sync responses.
* Good, because graceful degradation: if requested version too old (evicted from change log), returns error triggering full snapshot fallback.
* Good, because `RibChange` enum (Created, Updated, Deleted) captures all modification types with minimal overhead.
* Good, because change log operates independently of RIB persistence (snapshots)—logs track runtime changes for live sync.
* Good, because implementation is self-contained within RIB module without external dependencies (databases, message queues).
* Neutral, because bounded change log means very stale members (disconnected longer than retention window) require full sync.
* Bad, because change log is in-memory only—lost on IPCP restart (members reconnecting after restart need full sync).
* Bad, because circular buffer eviction (oldest changes removed when full) requires tracking `oldest_version` to detect "too old" requests.

### Always use full RIB snapshots for every synchronisation request

* Good, because it is simple to implement—serialize entire RIB and send via CDAP.
* Good, because no version tracking or change log management required.
* Good, because members always receive complete, consistent state without delta application complexity.
* Neutral, because full snapshots guarantee consistency regardless of synchronisation frequency or member downtime.
* Bad, because large RIBs (thousands of routes, flows, neighbours) produce multi-megabyte snapshots consuming significant bandwidth.
* Bad, because serialisation and deserialisation of full snapshots is CPU-intensive compared to small delta sets.
* Bad, because frequent synchronisation (every few seconds) wastes resources sending unchanged data repeatedly.
* Bad, because it does not scale to high-frequency updates or large DIFs with many members synchronising simultaneously.

### Use database-backed RIB with transaction log and SQL-based delta queries

* Good, because databases provide robust transaction logs with durability guarantees (changes survive IPCP restarts).
* Good, because SQL queries enable flexible delta retrieval (`SELECT * FROM rib_changes WHERE version > ?`).
* Good, because database indexing optimises version-based queries for large change histories.
* Good, because databases handle concurrent access with ACID properties, simplifying multi-actor scenarios.
* Neutral, because change retention is configurable via database policies (time-based or count-based pruning).
* Bad, because it introduces external dependency (SQLite, PostgreSQL) complicating deployment and adding failure modes.
* Bad, because database I/O overhead (disk writes per change) may reduce throughput compared to in-memory operations.
* Bad, because SQL query latency adds synchronisation delay compared to in-memory `VecDeque` lookups.
* Bad, because database schema evolution (migrations) complicates versioning and backwards compatibility.
* Bad, because embedded databases (SQLite) add binary size; external databases (PostgreSQL) require separate processes and configuration.

### Use event sourcing with complete change history persisted to disk

* Good, because complete history enables replaying changes from any point in time (debugging, audit trails).
* Good, because append-only log structure simplifies writes (no in-place updates, no corruption risk).
* Good, because event sourcing aligns with RINA's distributed coordination model (state derived from events).
* Good, because change history survives IPCP restarts, enabling precise recovery without full snapshots.
* Neutral, because log compaction (merging old events) required to prevent unbounded growth.
* Bad, because persistent storage (disk I/O) for every RIB change adds latency compared to in-memory logging.
* Bad, because complete history consumes significant disk space (megabytes to gigabytes for long-running IPCPs).
* Bad, because log replay for synchronisation can be slow if member is far behind (replaying thousands of events).
* Bad, because implementation complexity increases (log rotation, compaction, corruption recovery).

## More Information

### Current Implementation

`Rib` in [src/rib.rs](src/rib.rs) implements versioned objects with change logging:

#### Versioning

* **Version counter**: `Arc<RwLock<u64>>` generates monotonically increasing versions for all RIB operations.
* **Object versions**: Each `RibObject` has `version: u64` and `last_modified: u64` (Unix timestamp).
* **Version assignment**: Create, Update, Delete operations increment global counter and assign to affected object.

#### Change Log

* **Structure**: `RibChangeLog` wraps `Arc<RwLock<VecDeque<RibChange>>>` (bounded circular buffer, default 1000 entries).
* **Change types**:
  * `RibChange::Created(RibObject)`: New object with complete state
  * `RibChange::Updated(RibObject)`: Modified object with new value and incremented version
  * `RibChange::Deleted { name, version, timestamp }`: Removed object metadata
* **Eviction**: When full, `log_change()` removes oldest entry, updates `oldest_version` tracker.
* **Delta retrieval**: `get_changes_since(since_version)` filters changes by version, returns `Err` if requested version < `oldest_version`.

#### CDAP Integration

* **Sync request**: Member sends `CdapMessage::new_sync_request(invoke_id, last_known_version, requester)`.
* **Sync response**: Bootstrap calls `rib.get_changes_since(last_known_version)`:
  * **Success (incremental)**: `CdapMessage::new_sync_response(invoke_id, current_version, Some(changes), None, None)`
  * **Failure (too old)**: `CdapMessage::new_sync_response(invoke_id, current_version, None, Some(full_snapshot), Some(error))`
* **Application**: Member applies `RibChange` deltas: Created → `rib.create()`, Updated → `rib.update()`, Deleted → `rib.delete()`.

#### Operations

* **Create**: Increments version, creates `RibObject`, logs `RibChange::Created`, inserts into `objects` HashMap.
* **Update**: Increments version, modifies existing object, logs `RibChange::Updated`.
* **Delete**: Increments version, removes from HashMap, logs `RibChange::Deleted` with final version.
* **Current version**: `change_log.current_version()` returns latest change version (0 if empty).

### Design Rationale

* **Bounded log**: Limits memory usage whilst retaining sufficient history for typical synchronisation intervals (1000 changes ≈ several minutes of activity).
* **In-memory**: Avoids I/O overhead for high-frequency RIB operations (routing updates, flow state changes).
* **Graceful fallback**: Members detect "too old" errors and request full snapshots automatically.
* **Version monotonicity**: Single global counter ensures total ordering across all RIB changes, simplifying consistency reasoning.

### Conclusion

We choose versioned RIB with bounded change log for incremental CDAP synchronisation because it efficiently handles common scenarios (brief disconnections, periodic updates) whilst maintaining simplicity through in-memory operation. Version numbers provide total ordering, `RibChangeLog` tracks recent changes in a bounded circular buffer, and CDAP sync protocol seamlessly falls back to full snapshots when change history is insufficient. This approach balances efficiency (avoiding unnecessary full transfers), resource usage (bounded memory footprint), and operational simplicity (no external database dependencies) for distributed RIB consistency in RINA DIFs.
