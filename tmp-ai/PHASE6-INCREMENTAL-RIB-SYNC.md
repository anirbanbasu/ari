# Phase 6: Incremental RIB Synchronization

## Current Limitation

**Problem:** Full RIB snapshots are transferred during enrollment
- Bootstrap serializes entire RIB: `rib.serialize().await`
- Member deserializes entire snapshot: `rib.deserialize(&rib_data).await`
- Inefficient for large RIBs or frequent updates
- No mechanism for periodic synchronization after enrollment

**Example Scenario:**
```
Bootstrap RIB: 1000 routes, 500 flows, 200 neighbor entries
Member enrolls → Receives ~100KB snapshot
Another member joins → Bootstrap adds 1 route
First member needs that route → Must wait for re-enrollment or manual sync
No incremental update mechanism exists
```

## Design Goals

1. **Minimize bandwidth**: Send only changed objects, not entire RIB
2. **Version-based sync**: Use RIB version numbers to track changes
3. **Efficient queries**: Get changes since last known version
4. **Backward compatible**: Maintain full snapshot for initial enrollment
5. **Periodic sync**: Members periodically check for updates

## Architecture

### 1. RIB Change Log

Track all modifications to the RIB since a given version:

```rust
// src/rib.rs

/// Represents a single change to the RIB
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RibChange {
    Created(RibObject),
    Updated(RibObject),
    Deleted { name: String, version: u64, timestamp: u64 },
}

/// Change log for incremental synchronization
#[derive(Debug, Clone)]
pub struct RibChangeLog {
    /// Ordered list of changes (bounded by max_size)
    changes: Arc<RwLock<VecDeque<RibChange>>>,
    /// Maximum number of changes to retain
    max_size: usize,
    /// Oldest version available in change log
    oldest_version: Arc<RwLock<u64>>,
}

impl RibChangeLog {
    pub fn new(max_size: usize) -> Self {
        Self {
            changes: Arc::new(RwLock::new(VecDeque::with_capacity(max_size))),
            max_size,
            oldest_version: Arc::new(RwLock::new(0)),
        }
    }

    /// Add a change to the log
    pub async fn log_change(&self, change: RibChange) {
        let mut changes = self.changes.write().await;

        // Remove oldest if at capacity
        if changes.len() >= self.max_size {
            if let Some(removed) = changes.pop_front() {
                let version = match removed {
                    RibChange::Created(obj) => obj.version,
                    RibChange::Updated(obj) => obj.version,
                    RibChange::Deleted { version, .. } => version,
                };
                let mut oldest = self.oldest_version.write().await;
                *oldest = version + 1;
            }
        }

        changes.push_back(change);
    }

    /// Get all changes since a specific version
    pub async fn get_changes_since(&self, since_version: u64) -> Result<Vec<RibChange>, String> {
        let oldest = *self.oldest_version.read().await;

        // Check if requested version is too old
        if since_version < oldest {
            return Err(format!(
                "Requested version {} is too old. Oldest available: {}. Full sync required.",
                since_version, oldest
            ));
        }

        let changes = self.changes.read().await;
        Ok(changes
            .iter()
            .filter(|change| {
                let version = match change {
                    RibChange::Created(obj) => obj.version,
                    RibChange::Updated(obj) => obj.version,
                    RibChange::Deleted { version, .. } => *version,
                };
                version > since_version
            })
            .cloned()
            .collect())
    }

    /// Get the current version (latest change)
    pub async fn current_version(&self) -> u64 {
        let changes = self.changes.read().await;
        changes
            .back()
            .map(|change| match change {
                RibChange::Created(obj) => obj.version,
                RibChange::Updated(obj) => obj.version,
                RibChange::Deleted { version, .. } => *version,
            })
            .unwrap_or(0)
    }
}
```

### 2. Update RIB to Use Change Log

Modify `Rib` to track changes:

```rust
// src/rib.rs

pub struct Rib {
    objects: Arc<RwLock<HashMap<String, RibObject>>>,
    version_counter: Arc<RwLock<u64>>,
    // NEW: Change log for incremental sync
    change_log: RibChangeLog,
}

impl Rib {
    pub fn new() -> Self {
        Self {
            objects: Arc::new(RwLock::new(HashMap::new())),
            version_counter: Arc::new(RwLock::new(0)),
            change_log: RibChangeLog::new(1000), // Keep last 1000 changes
        }
    }

    pub async fn create(&self, name: String, class: String, value: RibValue) -> Result<(), String> {
        // ... existing create logic ...

        let obj = RibObject { /* ... */ };

        // Log the change
        self.change_log.log_change(RibChange::Created(obj.clone())).await;

        objects.insert(name, obj);
        Ok(())
    }

    pub async fn update(&self, name: &str, value: RibValue) -> Result<(), String> {
        // ... existing update logic ...

        // After updating object
        let updated_obj = objects.get(name).unwrap().clone();
        self.change_log.log_change(RibChange::Updated(updated_obj)).await;

        Ok(())
    }

    pub async fn delete(&self, name: &str) -> Result<(), String> {
        let mut objects = self.objects.write().await;

        match objects.remove(name) {
            Some(obj) => {
                // Log deletion with version info
                self.change_log.log_change(RibChange::Deleted {
                    name: obj.name,
                    version: obj.version,
                    timestamp: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
                }).await;
                Ok(())
            }
            None => Err(format!("Object '{}' not found", name)),
        }
    }

    /// Get changes since a specific version (for incremental sync)
    pub async fn get_changes_since(&self, since_version: u64) -> Result<Vec<RibChange>, String> {
        self.change_log.get_changes_since(since_version).await
    }

    /// Get current RIB version
    pub async fn current_version(&self) -> u64 {
        self.change_log.current_version().await
    }

    /// Apply incremental changes to RIB
    pub async fn apply_changes(&self, changes: Vec<RibChange>) -> Result<usize, String> {
        let mut applied = 0;

        for change in changes {
            match change {
                RibChange::Created(obj) => {
                    // Don't log this change (it came from remote)
                    let mut objects = self.objects.write().await;
                    objects.insert(obj.name.clone(), obj);
                    applied += 1;
                }
                RibChange::Updated(obj) => {
                    let mut objects = self.objects.write().await;
                    if let Some(existing) = objects.get_mut(&obj.name) {
                        if obj.version > existing.version {
                            *existing = obj;
                            applied += 1;
                        }
                    } else {
                        // Object doesn't exist locally, create it
                        objects.insert(obj.name.clone(), obj);
                        applied += 1;
                    }
                }
                RibChange::Deleted { name, .. } => {
                    let mut objects = self.objects.write().await;
                    if objects.remove(&name).is_some() {
                        applied += 1;
                    }
                }
            }
        }

        Ok(applied)
    }
}
```

### 3. New CDAP Messages for Sync

Add sync request/response to CDAP protocol:

```rust
// src/cdap.rs

/// Sync request message (sent by member to bootstrap)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncRequest {
    /// Last known RIB version on this member
    pub last_known_version: u64,
    /// Requesting IPCP name
    pub requester: String,
}

/// Sync response message (sent by bootstrap to member)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResponse {
    /// Current RIB version on bootstrap
    pub current_version: u64,
    /// Changes since requested version (None = full sync required)
    pub changes: Option<Vec<RibChange>>,
    /// Full snapshot (if changes is None)
    pub full_snapshot: Option<Vec<u8>>,
    /// Error message if sync failed
    pub error: Option<String>,
}
```

Update `CdapMessage` to support sync operations:

```rust
// src/cdap.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdapMessage {
    pub op_code: CdapOpCode,
    pub obj_name: String,
    pub obj_class: Option<String>,
    pub obj_value: Option<RibValue>,
    pub invoke_id: u64,
    pub result: i32,
    pub result_reason: Option<String>,

    // NEW: For sync operations
    pub sync_request: Option<SyncRequest>,
    pub sync_response: Option<SyncResponse>,
}
```

### 4. Update Enrollment Protocol

Modify enrollment to include version tracking:

```rust
// src/enrollment.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrollmentResponse {
    pub accepted: bool,
    pub error: Option<String>,
    pub assigned_address: Option<u64>,
    pub dif_name: String,
    pub rib_snapshot: Option<Vec<u8>>,

    // NEW: Include RIB version for future incremental syncs
    pub rib_version: u64,
}

impl EnrollmentManager {
    async fn handle_enrollment_request(&mut self, /* ... */) -> Result<(), EnrollmentError> {
        // ... existing code ...

        // Get current RIB version
        let rib_version = self.rib.current_version().await;

        let response = EnrollmentResponse {
            accepted: true,
            error: None,
            assigned_address,
            dif_name: dif_name.clone(),
            rib_snapshot,
            rib_version, // NEW: Send current version
        };

        // ... send response ...
    }

    async fn enrol_with_bootstrap(&mut self, /* ... */) -> Result<String, EnrollmentError> {
        // ... receive enrollment response ...

        // NEW: Store RIB version for future incremental syncs
        if let Some(rib_data) = enroll_response.rib_snapshot {
            let synced = self.rib.deserialize(&rib_data).await
                .map_err(|e| EnrollmentError::RibSyncFailed(e))?;

            println!("  ✓ Synchronized {} RIB objects", synced);
            println!("  ✓ RIB version: {}", enroll_response.rib_version);

            // Store version for incremental sync
            self.last_synced_version = enroll_response.rib_version;
        }

        // ... rest of enrollment ...
    }
}
```

### 5. Periodic Sync Task for Members

Add background task to periodically request updates:

```rust
// src/enrollment.rs

impl EnrollmentManager {
    /// Start periodic RIB synchronization task
    pub fn start_sync_task(
        self: Arc<Self>,
        sync_interval_secs: u64,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(
                tokio::time::Duration::from_secs(sync_interval_secs)
            );

            loop {
                interval.tick().await;

                if let Err(e) = self.sync_rib().await {
                    eprintln!("⚠️  RIB sync failed: {}", e);
                } else {
                    println!("✓ RIB sync completed");
                }
            }
        })
    }

    /// Request incremental RIB synchronization
    async fn sync_rib(&self) -> Result<(), EnrollmentError> {
        let last_version = self.last_synced_version;

        // Create sync request
        let sync_request = SyncRequest {
            last_known_version: last_version,
            requester: self.ipcp_name.clone().unwrap_or_default(),
        };

        // Send sync request via CDAP
        let cdap_msg = CdapMessage {
            op_code: CdapOpCode::Read,
            obj_name: "rib_sync".to_string(),
            obj_class: Some("sync".to_string()),
            obj_value: None,
            invoke_id: self.next_invoke_id(),
            result: 0,
            result_reason: None,
            sync_request: Some(sync_request),
            sync_response: None,
        };

        let pdu = self.create_sync_pdu(cdap_msg);

        // Send to bootstrap
        self.shim.send(self.bootstrap_addr, &pdu.serialize())
            .map_err(|e| EnrollmentError::NetworkError(e))?;

        // Wait for sync response
        let response = self.receive_sync_response().await?;

        // Apply changes or full snapshot
        match response.sync_response {
            Some(sync_resp) => {
                if let Some(error) = sync_resp.error {
                    return Err(EnrollmentError::RibSyncFailed(error));
                }

                if let Some(changes) = sync_resp.changes {
                    // Incremental sync
                    let applied = self.rib.apply_changes(changes).await
                        .map_err(|e| EnrollmentError::RibSyncFailed(e))?;

                    println!("  ✓ Applied {} incremental changes", applied);
                    self.last_synced_version = sync_resp.current_version;
                } else if let Some(snapshot) = sync_resp.full_snapshot {
                    // Full sync required (change log too old)
                    let synced = self.rib.deserialize(&snapshot).await
                        .map_err(|e| EnrollmentError::RibSyncFailed(e))?;

                    println!("  ✓ Full sync: {} objects", synced);
                    self.last_synced_version = sync_resp.current_version;
                } else {
                    // No changes
                    println!("  ✓ RIB up to date (version {})", last_version);
                }

                Ok(())
            }
            None => Err(EnrollmentError::InvalidResponse(
                "Missing sync response".to_string()
            )),
        }
    }
}
```

### 6. Bootstrap Sync Request Handler

Bootstrap handles sync requests from members:

```rust
// src/enrollment.rs (bootstrap side)

impl EnrollmentManager {
    async fn handle_sync_request(
        &self,
        sync_request: SyncRequest,
        src_addr: SocketAddr,
    ) -> Result<(), EnrollmentError> {
        println!(
            "  Sync request from {}: last_version={}",
            sync_request.requester, sync_request.last_known_version
        );

        let current_version = self.rib.current_version().await;

        // Try to get incremental changes
        let response = match self.rib.get_changes_since(sync_request.last_known_version).await {
            Ok(changes) => {
                if changes.is_empty() {
                    println!("  No changes since version {}", sync_request.last_known_version);
                } else {
                    println!("  Sending {} incremental changes", changes.len());
                }

                SyncResponse {
                    current_version,
                    changes: Some(changes),
                    full_snapshot: None,
                    error: None,
                }
            }
            Err(e) => {
                // Change log too old, send full snapshot
                println!("  Change log too old, sending full snapshot: {}", e);

                let snapshot = self.rib.serialize().await;

                SyncResponse {
                    current_version,
                    changes: None,
                    full_snapshot: Some(snapshot),
                    error: None,
                }
            }
        };

        // Send sync response
        let cdap_msg = CdapMessage {
            op_code: CdapOpCode::Read,
            obj_name: "rib_sync".to_string(),
            obj_class: Some("sync".to_string()),
            obj_value: None,
            invoke_id: 0, // Should match request invoke_id
            result: 0,
            result_reason: None,
            sync_request: None,
            sync_response: Some(response),
        };

        let pdu = self.create_sync_response_pdu(cdap_msg);
        self.shim.send(src_addr, &pdu.serialize())
            .map_err(|e| EnrollmentError::NetworkError(e))?;

        Ok(())
    }
}
```

## Configuration

Add sync settings to config:

```toml
# config/member.toml

[enrollment]
# ... existing enrollment settings ...

# RIB synchronization interval (seconds)
# Set to 0 to disable periodic sync (only sync on enrollment)
rib_sync_interval_secs = 30

# config/bootstrap.toml

[rib]
# Maximum number of changes to keep in change log
# Older changes require full snapshot sync
change_log_size = 1000
```

## Implementation Steps

### Step 1: RIB Change Log (1-2 hours)
- [ ] Create `RibChange` enum
- [ ] Implement `RibChangeLog` structure
- [ ] Update `Rib::create/update/delete` to log changes
- [ ] Add `get_changes_since()` and `apply_changes()` methods
- [ ] Unit tests for change log

### Step 2: CDAP Sync Messages (1 hour)
- [ ] Add `SyncRequest` and `SyncResponse` structures
- [ ] Update `CdapMessage` with sync fields
- [ ] Serialization tests

### Step 3: Enrollment Protocol Update (2 hours)
- [ ] Add `rib_version` to `EnrollmentResponse`
- [ ] Update bootstrap enrollment handler to include version
- [ ] Update member enrollment to store `last_synced_version`
- [ ] Integration test for versioned enrollment

### Step 4: Periodic Sync Task (2-3 hours)
- [ ] Implement `start_sync_task()` for members
- [ ] Implement `sync_rib()` request/response logic
- [ ] Add configuration for sync interval
- [ ] Handle sync failures gracefully

### Step 5: Bootstrap Sync Handler (1-2 hours)
- [ ] Implement `handle_sync_request()` on bootstrap
- [ ] Fallback to full snapshot when change log old
- [ ] Test incremental vs full sync scenarios

### Step 6: Integration Testing (2 hours)
- [ ] Test incremental sync with small changes
- [ ] Test full sync fallback when version too old
- [ ] Test periodic sync task
- [ ] Performance comparison: full vs incremental

## Benefits

**Bandwidth Savings:**
```
Scenario: 1000-object RIB, 5 routes added
Full sync: ~100KB transfer
Incremental sync: ~500 bytes (5 route objects)
Reduction: 99.5%
```

**Scalability:**
- Members can stay synchronized without re-enrollment
- Bootstrap doesn't resend unchanged data
- Change log bounded (configurable size)

**Production Ready:**
- Graceful fallback to full sync
- Version-based consistency
- Minimal configuration changes

## Testing Strategy

1. **Unit Tests**: Change log operations, version tracking
2. **Integration Tests**: Incremental sync vs full sync
3. **Performance Tests**: Compare bandwidth usage
4. **Chaos Tests**: Handle sync failures, outdated versions

## Migration Path

**Phase A:** Implement change log (backward compatible)
**Phase B:** Add sync messages and protocol
**Phase C:** Enable periodic sync for members
**Phase D:** Production deployment and monitoring

Total Implementation Time: **10-15 hours**
