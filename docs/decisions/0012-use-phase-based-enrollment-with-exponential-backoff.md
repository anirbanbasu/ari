---
parent: Decisions
nav_order: 12
# These are optional metadata elements. Feel free to remove any of them.
status: "accepted"
date: 2026-02-07
decision-makers:
    - abasu
---
# Use phase-based enrollment with exponential backoff

## Context and Problem Statement

RINA Inter-Process Communication Processes (IPCPs) must enroll with a Distributed IPC Facility (DIF) before they can participate in distributed operations. Network conditions (packet loss, congestion, temporary bootstrap unavailability) can cause enrollment requests to fail, leaving members unable to join the DIF. Single-shot enrollment attempts without retry logic force administrators to manually intervene or restart IPCPs, reducing system reliability and operational efficiency. We need a robust enrollment mechanism that handles transient failures gracefully whilst avoiding network overload from aggressive retry patterns.

## Considered Options

* Use single-shot enrollment with no retries.
* Use enrollment state machine with configurable timeout, retry attempts, and exponential backoff.
* Use manual retry by administrator via CLI or API.
* Use persistent enrollment queue with background worker processing retries indefinitely.

## Decision Outcome

Chosen option: "Use enrollment state machine with configurable timeout, retry attempts, and exponential backoff", because it provides reliable enrollment in the face of transient network failures whilst preventing resource exhaustion through bounded retries and exponentially increasing backoff intervals. The `EnrollmentState` enum tracks progress through phases (NotEnrolled, Initiated, Authenticating, Synchronizing, Enrolled, Failed), enabling observability and debugging. Configurable parameters (`timeout`, `max_retries`, `initial_backoff_ms`) allow tuning for different network environments—low-latency LANs use shorter timeouts and fewer retries, whilst high-latency WANs use longer timeouts and more retries with larger backoff intervals.

## Pros and Cons of the Options

### Use enrollment state machine with configurable timeout, retry attempts, and exponential backoff

* Good, because it automatically handles transient failures (packet loss, temporary bootstrap unavailability) without manual intervention.
* Good, because exponential backoff (1s, 2s, 4s, 8s) prevents network overload by spacing retry attempts increasingly far apart.
* Good, because configurable parameters (`EnrollmentConfig`) enable environment-specific tuning—LAN: 3 retries with 1s initial backoff, WAN: 5 retries with 2s initial backoff.
* Good, because `EnrollmentState` enum provides clear visibility into enrollment progress (NotEnrolled → Initiated → Authenticating → Synchronizing → Enrolled).
* Good, because failed enrollments transition to `Failed(String)` state with error details, enabling monitoring and alerting.
* Good, because bounded retries (`max_retries`, default 3) prevent infinite loops consuming resources.
* Good, because per-attempt timeout (`timeout`, default 5s) prevents indefinite blocking on unresponsive bootstrap IPCPs.
* Good, because implementation uses `tokio::time::timeout` and `tokio::time::sleep` for efficient async retry scheduling.
* Neutral, because exponential backoff increases total enrollment time (worst case: 1s + 2s + 4s = 7s for 3 retries with 1s initial backoff).
* Bad, because members experiencing persistent failures (e.g., bootstrap IPCP offline) will fail after `max_retries` attempts, requiring re-enrollment initiation.

### Use single-shot enrollment with no retries

* Good, because it is simple to implement—send request, wait for response, fail or succeed.
* Good, because single attempt has predictable timing (one timeout period, e.g., 5s).
* Good, because no retry logic means fewer code paths and simpler debugging.
* Neutral, because members can initiate enrollment again manually if first attempt fails.
* Bad, because transient failures (packet loss, brief network congestion) cause enrollment to fail unnecessarily.
* Bad, because administrators must monitor failures and manually trigger re-enrollment (operational burden).
* Bad, because no automatic recovery reduces system reliability—members remain offline during transient issues.
* Bad, because single timeout value must balance responsiveness (short timeout) vs. reliability (long timeout)—no good compromise exists.

### Use manual retry by administrator via CLI or API

* Good, because administrators have full control over retry timing and can inspect system state between attempts.
* Good, because manual control prevents automated retry storms during widespread failures (e.g., bootstrap IPCP crash).
* Good, because simple implementation—no retry logic in enrollment code.
* Neutral, because CLI/API provides flexibility for scripting custom retry strategies (e.g., external monitoring system triggers retries).
* Bad, because manual intervention increases operational burden—administrators must monitor failures and respond promptly.
* Bad, because delayed manual retries extend member downtime (minutes to hours vs. seconds for automatic retries).
* Bad, because human response time (minutes) is unsuitable for transient failures resolved within seconds.
* Bad, because 24/7 human monitoring is impractical for production systems—requires automated on-call or delays until business hours.

### Use persistent enrollment queue with background worker processing retries indefinitely

* Good, because persistent queue (e.g., database, file) survives IPCP restarts, enabling retry across process lifecycles.
* Good, because background worker decouples enrollment from application startup, preventing blocking.
* Good, because indefinite retries (with backoff) eventually succeed when bootstrap IPCP recovers from extended downtime.
* Good, because queue enables priority ordering (e.g., re-enrollment prioritized over initial enrollment).
* Neutral, because queue persistence adds complexity (database schema, file format, corruption handling).
* Bad, because indefinite retries consume resources (CPU, memory, network) even when bootstrap IPCP is permanently unavailable.
* Bad, because persistent storage dependency complicates deployment (database setup, file system permissions).
* Bad, because queue requires management (clearing stale entries, monitoring queue depth, preventing unbounded growth).
* Bad, because background worker adds concurrency complexity (queue access synchronization, worker lifecycle management).

## More Information

### Current Implementation

`EnrollmentManager` in [src/enrollment.rs](src/enrollment.rs) implements phase-based enrollment:

#### State Machine

* **States**: `EnrollmentState` enum with NotEnrolled, Initiated, Authenticating, Synchronizing, Enrolled, Failed(String).
* **Transitions**: NotEnrolled → Initiated (via `set_ipcp_name()`) → Authenticating (CDAP Create request sent) → Synchronizing (RIB snapshot received) → Enrolled (routes synced).
* **Failure**: Any phase can transition to Failed(error_message) on timeout or error.

#### Configuration

* **EnrollmentConfig**:
  * `timeout: Duration` (default 5s): Per-attempt timeout using `tokio::time::timeout`.
  * `max_retries: u32` (default 3): Maximum enrollment attempts before final failure.
  * `initial_backoff_ms: u64` (default 1000ms): Base backoff interval, doubled on each retry (exponential).

#### Retry Logic

* **Method**: `enrol_with_bootstrap(bootstrap_addr)` loops from attempt 1 to `max_retries`.
* **Timeout**: Each attempt wrapped in `tokio::time::timeout(config.timeout, try_enrol(bootstrap_addr))`.
* **Backoff**: Failed attempts sleep for `initial_backoff_ms * 2^(attempt-1)` milliseconds (1s → 2s → 4s).
* **Success**: Returns `Ok(dif_name)` immediately on successful enrollment, skipping remaining retries.
* **Failure**: Returns `Err(EnrollmentError::Timeout{attempts: max_retries})` after exhausting retries.

#### Enrollment Phases

1. **Initiated**: IPCP name set, ready to begin enrollment.
2. **Authenticating**: `EnrollmentRequest` serialized to `CdapMessage::Create`, sent via UDP shim.
3. **Synchronizing**: `EnrollmentResponse` received, RIB snapshot deserialized via `rib.deserialize()`.
4. **Enrolled**: State updated, DIF name stored in RIB, routing table synced via CDAP Read.

### Design Rationale

* **Exponential backoff**: Prevents retry storms under sustained failures (e.g., bootstrap IPCP offline), reducing network congestion compared to fixed intervals.
* **Bounded retries**: Limits resource consumption whilst providing sufficient resilience for transient failures (packet loss typically recovers within 1-3 retries).
* **Async implementation**: Uses `tokio::time::timeout` and `tokio::time::sleep` for non-blocking retries, enabling concurrent operations (heartbeat monitoring, CDAP message handling).
* **State observability**: `EnrollmentState` enum enables monitoring via `state()` method, supporting operational dashboards and alerting.

### Conclusion

We choose phase-based enrollment with exponential backoff to provide robust, automatic recovery from transient network failures whilst preventing resource exhaustion through bounded retries and exponentially increasing backoff intervals. The `EnrollmentState` enum provides clear visibility into enrollment progress, and configurable `EnrollmentConfig` parameters enable tuning for diverse network environments. This approach balances reliability (automatic retries), efficiency (exponential backoff), and operational simplicity (no manual intervention required) for RINA DIF membership management.
