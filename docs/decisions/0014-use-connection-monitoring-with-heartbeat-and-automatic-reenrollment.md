---
parent: Decisions
nav_order: 14
# These are optional metadata elements. Feel free to remove any of them.
status: "proposed"
date: 2026-02-07
decision-makers:
    - abasu
---
# Use connection monitoring with heartbeat and automatic re-enrollment

## Context and Problem Statement

RINA Inter-Process Communication Processes (IPCPs) enrolled in a Distributed IPC Facility (DIF) can lose connectivity to the bootstrap IPCP due to network partitions, bootstrap restarts, or transient failures. Members unaware of connection loss continue attempting to communicate, resulting in failed Protocol Data Unit transmissions and degraded application performance. Manual administrator intervention to detect disconnections and trigger re-enrollment is operationally inefficient and introduces unacceptable downtime (minutes to hours). We need an automated connection monitoring mechanism that detects connectivity loss promptly and initiates re-enrollment without human intervention.

## Considered Options

* Use manual reconnection triggered by administrator via Command-Line Interface (CLI) or Application Programming Interface (API).
* Use background task monitors heartbeat intervals and triggers re-enrollment on connection loss.
* Use network-layer keepalives (Transmission Control Protocol (TCP) keepalive, User Datagram Protocol (UDP) session tracking).
* Use passive monitoring only—detect disconnection on failed sends, no proactive checking.

## Decision Outcome

Chosen option: "Use background task monitors heartbeat intervals and triggers re-enrollment on connection loss", because it provides proactive connection monitoring and automatic recovery without relying on network-layer mechanisms or manual intervention. The `EnrollmentConfig` includes `heartbeat_interval_secs` (default 30s) and `connection_timeout_secs` (default 90s). A background task spawned via `start_connection_monitoring()` checks `last_heartbeat` timestamp every `heartbeat_interval_secs / 2` seconds—if elapsed time exceeds `connection_timeout_secs`, the task automatically triggers re-enrollment with the bootstrap IPCP. This approach ensures members recover from connectivity loss within seconds to minutes rather than requiring manual intervention.

## Pros and Cons of the Options

### Use background task monitors heartbeat intervals and triggers re-enrollment on connection loss

* Good, because proactive monitoring detects connection loss promptly (within `connection_timeout_secs`), enabling fast recovery.
* Good, because automatic re-enrollment eliminates manual intervention—members self-heal without administrator action.
* Good, because configurable intervals (`heartbeat_interval_secs`, `connection_timeout_secs`) enable environment-specific tuning—stable networks use longer intervals (60s heartbeat, 180s timeout), unstable networks use shorter intervals (15s heartbeat, 45s timeout).
* Good, because background task (`tokio::spawn`) runs independently of enrollment operations, avoiding blocking.
* Good, because heartbeat timestamp (`last_heartbeat: Arc<RwLock<Option<Instant>>>`) updated on any received message from bootstrap, not just dedicated heartbeat packets—reducing network overhead.
* Good, because re-enrollment flag (`re_enrollment_in_progress: Arc<RwLock<bool>>`) prevents concurrent re-enrollment attempts during prolonged outages.
* Good, because monitoring can be disabled (`heartbeat_interval_secs = 0`) for scenarios where manual control preferred (testing, debugging).
* Good, because `is_connection_healthy()` method exposes connectivity status to applications for health checks and monitoring.
* Neutral, because background task checks at `heartbeat_interval_secs / 2` frequency—balance between responsiveness and CPU overhead.
* Bad, because connection timeout detection is passive—actual disconnection detected only after `connection_timeout_secs` elapsed (e.g., 90s delay before re-enrollment begins).
* Bad, because background task lifecycle requires management—task handle must be stored/aborted on IPCP shutdown to prevent resource leaks.

### Use manual reconnection triggered by administrator via CLI or API

* Good, because administrator has full control over reconnection timing—can inspect logs, verify bootstrap availability before triggering.
* Good, because no background task overhead—connections checked only when administrator initiates.
* Good, because manual control prevents automated retry storms during bootstrap maintenance windows.
* Neutral, because CLI/API enables scripting via external monitoring systems (Prometheus alerts trigger reconnection).
* Bad, because manual intervention increases operational burden—requires 24/7 monitoring and rapid administrator response.
* Bad, because human response time (minutes to hours) far exceeds acceptable downtime for real-time applications.
* Bad, because administrator must detect disconnections through external monitoring—no built-in connection health visibility.
* Bad, because manual reconnection delays extend application-level failures—users experience degraded service until reconnection.

### Use network-layer keepalives (TCP keepalive, UDP session tracking)

* Good, because network-layer mechanisms (TCP keepalive probes, UDP socket timeouts) detect link-level failures automatically.
* Good, because leveraging existing network stack features reduces application-layer code complexity.
* Good, because TCP keepalive parameters (probe interval, retry count) configurable via socket options (`SO_KEEPALIVE`, `TCP_KEEPIDLE`).
* Neutral, because UDP lacks built-in keepalive—requires custom session tracking or switching to TCP shim.
* Bad, because network-layer keepalives detect transport failures, not RINA-level enrollment state—bootstrap could be reachable but not accepting enrollments.
* Bad, because TCP keepalive intervals typically coarse-grained (Linux default: 7200s until first probe)—unsuitable for prompt failure detection.
* Bad, because network-layer failures trigger socket closure, not automatic re-enrollment—application must detect closed sockets and reinitiate enrollment.
* Bad, because reliance on network-layer keepalives ties RINA implementation to specific transport characteristics, reducing portability across shim layers.

### Use passive monitoring only—detect disconnection on failed sends, no proactive checking

* Good, because passive monitoring has zero overhead when connections healthy—no periodic checks or background tasks.
* Good, because failed send detection (`send_pdu()` returns error) is immediate—no delay waiting for timeout.
* Good, because simple implementation—trigger re-enrollment on any send failure without additional monitoring code.
* Neutral, because send failures provide definitive proof of connectivity loss (vs. heartbeat timeouts which may be false positives).
* Bad, because disconnections detected only when application attempts communication—idle members remain unaware of connection loss.
* Bad, because first send after disconnection fails, causing application-level error—proactive monitoring avoids this by re-enrolling before sends.
* Bad, because send failures may be transient (temporary congestion, packet loss)—distinguishing permanent disconnection from transient failures requires retry logic.
* Bad, because passive detection delays recovery—if application sends infrequently (e.g., every 5 minutes), disconnection remains undetected for up to 5 minutes.

## More Information

### Current Implementation

`EnrollmentManager` in [src/enrollment.rs](src/enrollment.rs) implements connection monitoring:

#### Configuration

* **EnrollmentConfig**:
  * `heartbeat_interval_secs: u64` (default 30s): Interval between heartbeat checks (0 disables monitoring).
  * `connection_timeout_secs: u64` (default 90s): Time without heartbeat before triggering re-enrollment.

#### Heartbeat Tracking

* **State**: `last_heartbeat: Arc<RwLock<Option<Instant>>>` tracks last message received from bootstrap.
* **Update**: `update_heartbeat()` sets timestamp to `Instant::now()`, called on any CDAP message reception (enrollment response, RIB sync, etc.).
* **Bootstrap initialization**: Bootstrap IPCP sets `last_heartbeat = Some(Instant::now())` at creation (always healthy).
* **Member initialization**: Member IPCP starts with `last_heartbeat = None`, set after successful enrollment.

#### Monitoring Task

* **Launch**: `start_connection_monitoring()` spawns `tokio::task`, returns `JoinHandle` for lifecycle management.
* **Check interval**: Task sleeps `heartbeat_interval_secs / 2` seconds between checks (e.g., 15s for 30s heartbeat).
* **Timeout detection**: Compares `last_heartbeat.elapsed()` against `connection_timeout_secs`—if exceeded, triggers re-enrollment.
* **Re-enrollment logic**:
  1. Check `re_enrollment_in_progress` flag (skip if already re-enrolling).
  2. Create temporary `EnrollmentManager` with current configuration.
  3. Call `enrol_with_bootstrap(bootstrap_addr)` (uses exponential backoff from ADR 0012).
  4. On success: Update `last_heartbeat`, clear `re_enrollment_in_progress`.
  5. On failure: Log error, clear `re_enrollment_in_progress` (monitoring task will retry after next timeout).

#### Health Checking

* **Method**: `is_connection_healthy()` returns `true` if `last_heartbeat.elapsed() < connection_timeout_secs`.
* **Manual re-enrollment**: `re_enroll()` method triggers immediate re-enrollment (bypasses monitoring task).

### Design Rationale

* **Proactive monitoring**: Detects disconnections before application sends, avoiding user-visible failures.
* **Piggyback heartbeats**: Uses existing CDAP messages (sync requests, responses) as implicit heartbeats, eliminating dedicated heartbeat protocol overhead.
* **Configurable timeouts**: Different environments have different failure detection requirements—short timeouts for low-latency LANs (30s/90s), long timeouts for high-latency WANs (120s/360s).
* **Independent task**: Background monitoring task runs concurrently with application logic, using `tokio` async runtime for efficient scheduling.
* **Retry integration**: Re-enrollment uses ADR 0012 exponential backoff, handling transient bootstrap unavailability during recovery.

### Conclusion

We choose connection monitoring with heartbeat and automatic re-enrollment to enable proactive detection of connectivity loss and self-healing recovery without manual intervention. The background monitoring task checks `last_heartbeat` timestamp periodically (every `heartbeat_interval_secs / 2` seconds) and triggers re-enrollment when elapsed time exceeds `connection_timeout_secs`. This approach balances responsiveness (prompt failure detection), efficiency (minimal network overhead via message piggybacking), and operational simplicity (no administrator intervention required) for RINA DIF membership resilience.
