---
parent: Decisions
nav_order: 8
# These are optional metadata elements. Feel free to remove any of them.
status: "accepted"
date: 2026-02-07
decision-makers:
    - abasu
---
# Use separate bootstrap and member operational modes for Inter-Process Communication Processes

## Context and Problem Statement

RINA requires a Distributed IPC Facility (DIF) to have at least one Inter-Process Communication Process (IPCP) that creates and manages the DIF infrastructure, including address allocation for new members. Subsequent IPCPs must enrol with an existing DIF member to obtain an address and join the DIF. We need to decide how to structure IPCP initialisation: should all IPCPs operate symmetrically in a peer-to-peer manner, use distinct operational modes with different responsibilities, or employ a centralised controller architecture?

## Considered Options

* Separate bootstrap and member operational modes with distinct initialisation paths and responsibilities.
* Peer-to-peer without designated bootstrap using dynamic leader election.
* Centralised controller separate from IPCPs managing all coordination.
* Hybrid mode allowing IPCPs to switch between bootstrap and member roles dynamically.
* Single operational mode with capability negotiation determining roles at runtime.

## Decision Outcome

Chosen option: "Separate bootstrap and member operational modes with distinct initialisation paths and responsibilities", because it provides clear separation of concerns, predictable behaviour during DIF initialisation, and explicit configuration of the first IPCP's responsibilities. Bootstrap IPCPs have a pre-configured static address and manage address allocation for members, whilst member IPCPs enrol with bootstrap peers to request dynamic address assignment. This design simplifies deployment, avoids distributed consensus complexity for initial DIF creation, and aligns with traditional RINA reference implementations.

## Pros and Cons of the Options

### Separate bootstrap and member operational modes with distinct initialisation paths and responsibilities

* Good, because it provides explicit configuration declaring which IPCP creates the DIF (`mode = "bootstrap"` in TOML).
* Good, because the bootstrap IPCP is pre-enrolled with a static address, avoiding circular dependency (cannot enrol without an existing member).
* Good, because bootstrap manages an address pool (configurable range, e.g., 1002-1999) and allocates addresses to enrolling members via `AddressPool::allocate()`.
* Good, because member IPCPs have clear bootstrap peer configuration (`bootstrap_peers = [{ address, rina_addr }]`) for enrolment.
* Good, because separation enables different initialisation logic: bootstrap loads persistence snapshots first, whilst members perform enrolment protocol.
* Good, because it simplifies operational reasoning: operators know the bootstrap IPCP is the authority for address allocation.
* Good, because it matches RINA reference architectures where the first IPCP has distinct responsibilities.
* Neutral, because it requires explicit mode configuration, though this is easily expressed in TOML files.
* Bad, because bootstrap becomes a single point of failure for address allocation (mitigated by future bootstrap redundancy).
* Bad, because it prevents dynamic reconfiguration of the bootstrap role without restarting the IPCP.

### Peer-to-peer without designated bootstrap using dynamic leader election

* Good, because it avoids single points of failure by distributing bootstrap responsibilities.
* Good, because it enables fully symmetric peer relationships after initial DIF creation.
* Good, because it allows dynamic failover if the original bootstrap IPCP becomes unavailable.
* Neutral, because leader election requires distributed consensus protocols (Raft, Paxos) adding complexity.
* Bad, because it introduces non-deterministic behaviour during initialisation (which peer becomes bootstrap?).
* Bad, because distributed consensus algorithms add significant implementation complexity and potential for split-brain scenarios.
* Bad, because election timeouts and convergence delays slow DIF initialisation compared to pre-configured bootstrap.
* Bad, because multiple IPCPs starting simultaneously may all attempt to create the DIF, requiring conflict resolution.
* Bad, because it complicates address allocation: peers must agree on non-overlapping address ranges during election.

### Centralised controller separate from IPCPs managing all coordination

* Good, because it provides a single source of truth for DIF membership and address allocation.
* Good, because the controller can enforce global policies across all IPCPs.
* Good, because IPCPs become stateless clients, simplifying their implementation.
* Neutral, because the controller can be made highly available with standard techniques (clustering, failover).
* Bad, because it introduces an external dependency outside the RINA architecture, violating RINA's recursive principle.
* Bad, because it creates a bottleneck: all coordination goes through the controller.
* Bad, because it adds deployment complexity (separate controller process, additional configuration).
* Bad, because it contradicts RINA philosophy where IPCPs are self-sufficient and recursive.
* Bad, because controller failure halts all DIF operations, creating a critical single point of failure.

### Hybrid mode allowing IPCPs to switch between bootstrap and member roles dynamically

* Good, because it enables operational flexibility: any member could become bootstrap on demand.
* Good, because it supports bootstrap failover by promoting members to bootstrap role.
* Good, because it reduces configuration burden (no need to designate bootstrap at startup).
* Neutral, because role transitions require state synchronisation (address pool migration, RIB handoff).
* Bad, because it significantly increases implementation complexity with role transition logic.
* Bad, because state migration during role changes is error-prone (address pool must be transferred atomically).
* Bad, because it complicates testing: must verify correct behaviour during all role transition scenarios.
* Bad, because it creates ambiguity about which IPCP is authoritative at any given time.
* Bad, because current use cases do not require dynamic role switching, making this premature optimisation.

### Single operational mode with capability negotiation determining roles at runtime

* Good, because it provides maximum flexibility without hardcoded operational modes.
* Good, because it allows organic emergence of roles based on DIF state at startup.
* Good, because all IPCPs have identical code paths, simplifying implementation.
* Neutral, because capability negotiation protocol must be designed and tested.
* Bad, because runtime role determination is non-deterministic, complicating operations and debugging.
* Bad, because it requires complex negotiation protocol to decide which IPCP becomes bootstrap.
* Bad, because race conditions during capability negotiation can lead to multiple IPCPs claiming bootstrap role.
* Bad, because troubleshooting is harder: operators cannot know from configuration which IPCP will manage addresses.
* Bad, because it delays initialisation whilst IPCPs negotiate roles.

## More Information

### Current Implementation

ARI implements two distinct operational modes configured via TOML:

#### Bootstrap Mode

* **Configuration**: Set `mode = "bootstrap"` in `[ipcp]` section of TOML configuration.
* **Static address**: Bootstrap IPCP has a pre-configured RINA address (e.g., `address = 1001` in `[dif]` section).
* **Enrolment state**: Bootstrap starts in `EnrollmentState::Enrolled`, skipping enrolment protocol since it creates the DIF.
* **Address allocation**: Bootstrap manages an `AddressPool` with configurable range (`address_pool_start = 1002`, `address_pool_end = 1999`).
* **Initialisation**:
  * Loads RIB snapshot if persistence is enabled (`rib_snapshot_path`)
  * Loads static routes from configuration into RIB (`[[routing.static_routes]]`)
  * Initialises `RouteResolver` with dynamic route persistence settings
  * Spawns actor tasks (RibActor, EfcpActor, RmtActor)
  * Binds shim to UDP socket (`bind_address`, `bind_port`)
  * Creates `EnrollmentManager::new_bootstrap()` with address pool
  * Enters listening loop waiting for enrolment requests from members
* **Responsibilities**:
  * Accept enrolment requests from member IPCPs via CDAP `Create` messages
  * Allocate addresses from pool to members requesting dynamic assignment (`request_address = true`)
  * Send enrolment responses with assigned address and DIF name
  * Create dynamic routes to enrolled members in `RouteResolver`
  * Persist RIB and route snapshots periodically

#### Member Mode

* **Configuration**: Set `mode = "member"` in `[ipcp]` section of TOML configuration.
* **Dynamic address**: Member typically starts with `address = 0` to request dynamic assignment during enrolment.
* **Bootstrap peers**: Member configuration includes `bootstrap_peers = [{ address = "host:port", rina_addr = 1001 }]` in `[enrollment]` section.
* **Enrolment state**: Member starts in `EnrollmentState::NotEnrolled`, transitions through `Initiated` → `Enrolling` → `Enrolled`.
* **Initialisation**:
  * Initialises RIB (does **not** load route persistence snapshots—members learn routes from bootstrap)
  * Loads static routes from configuration into RIB
  * Spawns actor tasks (RibActor, EfcpActor, RmtActor)
  * Binds shim to UDP socket
  * Creates `EnrollmentManager::new()` (without address pool)
  * Initiates enrolment with bootstrap via `enrol_with_bootstrap(bootstrap_rina_addr)`
* **Enrolment protocol**:
  * Creates `EnrollmentRequest` with `ipcp_name`, `ipcp_address`, `request_address = true`
  * Serialises request as CDAP `Create` message with `obj_class = "enrollment"`
  * Sends request PDU to bootstrap peer via shim
  * Waits for `EnrollmentResponse` with assigned address and DIF name
  * Updates `local_addr` with assigned address
  * Loads RIB snapshot from response if provided
* **Timeout and retry**:
  * Configurable timeout (`timeout_secs`, default 5 seconds)
  * Exponential backoff retries (`max_retries`, `initial_backoff_ms`)
  * Implements `tokio::time::timeout()` to prevent indefinite blocking
* **Post-enrolment**:
  * Transitions IPCP state to `IpcpState::Operational`
  * Periodically sends heartbeats to bootstrap (future work)
  * Re-enrols if heartbeat timeout is detected (future work)

### Design Rationale

The bootstrap/member distinction addresses several fundamental challenges in RINA DIF initialisation:

#### Circular Dependency Resolution

* **Problem**: Cannot enrol without an existing DIF member, but the first member must exist to create the DIF.
* **Solution**: Bootstrap is pre-enrolled (`EnrollmentState::Enrolled` at startup), breaking the circular dependency.

#### Address Allocation Authority

* **Problem**: Dynamic address assignment requires a central allocator to prevent conflicts.
* **Solution**: Bootstrap holds the `AddressPool` and is the sole authority for address assignment during enrolment.

#### Operational Clarity

* **Problem**: Distributed systems with symmetric peers require complex coordination.
* **Solution**: Explicit configuration (`mode = "bootstrap"` vs `mode = "member"`) makes operational intent clear.

#### Configuration Simplicity

* **Problem**: Dynamic role negotiation adds runtime complexity and failure modes.
* **Solution**: TOML files declare mode statically, enabling validation before process start.

### Alternatives for Specific Use Cases

#### Multiple Bootstrap IPCPs (Future Work)

* Use case: High availability for address allocation and enrolment services.
* Approach: Multiple bootstrap IPCPs with partitioned address pools (e.g., bootstrap-1 manages 1002-1500, bootstrap-2 manages 1501-1999).
* Configuration: Members specify multiple `bootstrap_peers` for failover.
* Requires: Distributed address pool coordination or static partitioning strategy.

#### Bootstrap Promotion (Future Work)

* Use case: Member IPCP takes over bootstrap responsibilities if original bootstrap fails.
* Approach: Member transitions to bootstrap mode via explicit operator command or automatic failover trigger.
* Requires: Address pool state migration, RIB synchronisation, reconfiguration of dependent members.

#### Ephemeral Bootstrap (Development/Testing)

* Use case: Temporary bootstrap for local testing without persistent configuration.
* Approach: Command-line mode (`--mode bootstrap --address 1001 --bind 0.0.0.0:7000`) without TOML file.
* Current status: Supported via `CliArgs` with fallback defaults.

### Tooling and Deployment

#### Configuration Generation

* **Bootstrap template**: [config/bootstrap.toml](config/bootstrap.toml) demonstrates required sections and defaults.
* **Member template**: [config/member.toml](config/member.toml) shows enrolment peer configuration.
* Validation: `IpcpConfiguration::from_file()` enforces mode-specific requirements (e.g., bootstrap requires `address`).

#### Command-Line Invocation

```bash
# Bootstrap IPCP
cargo run --release -- --config config/bootstrap.toml

# Member IPCP
cargo run --release -- --config config/member.toml
```

#### Monitoring and Debugging

* Bootstrap logs: "Waiting for enrollment requests from member IPCPs..."
* Member logs: "Enrollment attempt 1/3", "Successfully enrolled in DIF: production-dif", "Assigned address: 1002"
* Address pool tracking: Bootstrap logs allocated addresses and available count.

### Migration and Evolution

#### Current Limitations

* Single bootstrap: No high availability for address allocation.
* Static mode: Cannot transition between bootstrap and member at runtime.
* No bootstrap discovery: Members must have pre-configured bootstrap peer addresses.

#### Future Enhancements

* **Bootstrap redundancy**: Multiple bootstrap IPCPs with consistent address allocation (requires distributed state agreement).
* **Dynamic bootstrap discovery**: Service discovery protocol for members to locate bootstrap peers automatically.
* **Role transition**: Explicit operator command to promote member to bootstrap or demote bootstrap to member (requires state migration protocol).
* **Hierarchical addressing**: Bootstrap manages address prefix delegation to sub-DIFs (aligns with RINA recursion principle).

### Conclusion

We choose separate bootstrap and member operational modes because they provide clear, predictable initialisation paths for RINA DIFs whilst avoiding the complexity of distributed consensus for initial DIF creation. Bootstrap IPCPs have explicit configuration with static addresses and address pool management, creating the DIF foundation. Member IPCPs have explicit enrolment peer configuration and request dynamic address assignment during enrolment. This design mirrors traditional RINA reference implementations, simplifies operational deployment, and provides a stable foundation for future enhancements such as bootstrap redundancy and dynamic role transitions. The explicit mode distinction makes configuration validation straightforward, enables early detection of misconfiguration, and ensures deterministic behaviour during DIF initialisation.
