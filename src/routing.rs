// SPDX-License-Identifier: EUPL-1.2-or-later
// Copyright © 2026-present ARI Contributors

//! Route resolution and management with optional persistence
//!
//! This module abstracts the next-hop resolution logic from the RMT,
//! providing a clean interface for route lookups and dynamic route management.
//!
//! # Features
//! - Hybrid routing: Static (config-driven) + Dynamic (learned during enrollment)
//! - Optional persistence: Save/load dynamic routes to/from TOML snapshots
//! - TTL-based expiration: Automatic stale route detection
//! - Validation on load: Filter expired routes during startup
//! - Periodic snapshots: Background task saves routes at configured intervals

use crate::error::AriError;
use crate::rib::{Rib, RibValue};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tokio::time::{Duration, interval};

/// Metadata for a dynamic route entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteMetadata {
    /// RINA address of the destination
    pub destination: u64,
    /// Next-hop socket address (UDP underlay)
    pub next_hop_address: String,
    /// Unix timestamp when the route was created (seconds since epoch)
    pub created_at: u64,
    /// Time-to-live in seconds (0 = never expires)
    pub ttl_seconds: u64,
}

impl RouteMetadata {
    /// Check if the route has expired
    pub fn is_expired(&self) -> bool {
        if self.ttl_seconds == 0 {
            return false; // Never expires
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let age = now.saturating_sub(self.created_at);
        age > self.ttl_seconds
    }

    /// Get the remaining TTL in seconds
    pub fn remaining_ttl(&self) -> u64 {
        if self.ttl_seconds == 0 {
            return u64::MAX; // Never expires
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let age = now.saturating_sub(self.created_at);
        self.ttl_seconds.saturating_sub(age)
    }
}

/// Snapshot of dynamic routes for persistence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteSnapshot {
    /// Version for future compatibility
    pub version: u32,
    /// Timestamp of snapshot creation
    pub snapshot_time: u64,
    /// Dynamic routes with metadata
    pub routes: Vec<RouteMetadata>,
}

impl RouteSnapshot {
    /// Create a new snapshot from current routes
    pub fn new(routes: Vec<RouteMetadata>) -> Self {
        let snapshot_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            version: 1,
            snapshot_time,
            routes,
        }
    }

    /// Load snapshot from TOML file
    pub fn load_from_file(path: &PathBuf) -> Result<Self, AriError> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            AriError::Rib(crate::error::RibError::OperationFailed(format!(
                "Failed to read file: {}",
                e
            )))
        })?;

        let snapshot: RouteSnapshot = toml::from_str(&content).map_err(|e| {
            AriError::Rib(crate::error::RibError::OperationFailed(format!(
                "Failed to parse TOML: {}",
                e
            )))
        })?;

        Ok(snapshot)
    }

    /// Save snapshot to TOML file
    pub fn save_to_file(&self, path: &PathBuf) -> Result<(), AriError> {
        let content = toml::to_string_pretty(self).map_err(|e| {
            AriError::Rib(crate::error::RibError::OperationFailed(format!(
                "Failed to serialize: {}",
                e
            )))
        })?;

        // Create parent directories if they don't exist
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                AriError::Rib(crate::error::RibError::OperationFailed(format!(
                    "Failed to create directory {:?}: {}",
                    parent, e
                )))
            })?;
        }

        std::fs::write(path, content).map_err(|e| {
            AriError::Rib(crate::error::RibError::OperationFailed(format!(
                "Failed to write file: {}",
                e
            )))
        })?;

        Ok(())
    }

    /// Filter out expired routes
    pub fn filter_valid(&self) -> Vec<RouteMetadata> {
        self.routes
            .iter()
            .filter(|r| !r.is_expired())
            .cloned()
            .collect()
    }
}

/// Configuration for route resolution
#[derive(Debug, Clone)]
pub struct RouteResolverConfig {
    /// Enable persistence of dynamic routes
    pub enable_persistence: bool,
    /// Path to snapshot file
    pub snapshot_path: PathBuf,
    /// Default TTL for new dynamic routes (seconds, 0 = never expires)
    pub default_ttl_seconds: u64,
    /// Interval between automatic snapshots (seconds)
    pub snapshot_interval_seconds: u64,
}

impl Default for RouteResolverConfig {
    fn default() -> Self {
        Self {
            enable_persistence: false,
            snapshot_path: PathBuf::from("dynamic-routes.toml"),
            default_ttl_seconds: 3600,      // 1 hour default
            snapshot_interval_seconds: 300, // 5 minutes
        }
    }
}

/// Route resolver abstracts next-hop lookups and dynamic route management
#[derive(Debug)]
pub struct RouteResolver {
    /// Reference to the RIB for static route lookups
    rib: Arc<RwLock<Rib>>,
    /// Configuration for persistence and TTL
    config: RouteResolverConfig,
    /// Cache of dynamic route metadata for efficient TTL checks
    metadata_cache: Arc<RwLock<HashMap<u64, RouteMetadata>>>,
}

impl RouteResolver {
    /// Create a new route resolver
    pub fn new(rib: Arc<RwLock<Rib>>, config: RouteResolverConfig) -> Self {
        Self {
            rib,
            config,
            metadata_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Resolve the next-hop socket address for a destination RINA address
    ///
    /// Lookup order:
    /// 1. Static routes (highest priority)
    /// 2. Dynamic routes (check TTL expiration)
    /// 3. Error if no route found
    pub async fn resolve_next_hop(&self, dst_addr: u64) -> Result<SocketAddr, AriError> {
        // Try static route first (highest priority)
        let static_route_name = format!("/routing/static/{}", dst_addr);
        let rib = self.rib.read().await;

        if let Some(obj) = rib.read(&static_route_name).await
            && let RibValue::Struct(fields) = &obj.value
            && let Some(socket_addr_box) = fields.get("next_hop_address")
            && let RibValue::String(socket_addr) = socket_addr_box.as_ref()
        {
            return socket_addr.parse().map_err(|e| {
                AriError::Rmt(crate::error::RmtError::Network(format!(
                    "Invalid socket address: {}",
                    e
                )))
            });
        }

        // Try dynamic route (check TTL)
        let dynamic_route_name = format!("/routing/dynamic/{}", dst_addr);
        if let Some(obj) = rib.read(&dynamic_route_name).await
            && let RibValue::Struct(fields) = &obj.value
        {
            // Check if route has expired
            let metadata_cache = self.metadata_cache.read().await;
            if let Some(metadata) = metadata_cache.get(&dst_addr)
                && metadata.is_expired()
            {
                // Route expired - remove it
                drop(metadata_cache);
                drop(rib);
                self.remove_dynamic_route(dst_addr).await?;
                return Err(AriError::Rmt(crate::error::RmtError::RouteNotFound(
                    dst_addr,
                )));
            }

            // Route is valid
            if let Some(socket_addr_box) = fields.get("next_hop_address")
                && let RibValue::String(socket_addr) = socket_addr_box.as_ref()
            {
                return socket_addr.parse().map_err(|e| {
                    AriError::Rmt(crate::error::RmtError::Network(format!(
                        "Invalid socket address: {}",
                        e
                    )))
                });
            }
        }

        // No route found
        Err(AriError::Rmt(crate::error::RmtError::RouteNotFound(
            dst_addr,
        )))
    }

    /// Add a dynamic route (typically during enrollment)
    ///
    /// This method is idempotent - if a route already exists for the destination,
    /// it will be updated with the new next-hop information. This handles re-enrollment
    /// scenarios where a member rejoins after a crash or network issue.
    pub async fn add_dynamic_route(
        &self,
        dst_addr: u64,
        next_hop: SocketAddr,
        ttl_seconds: Option<u64>,
    ) -> Result<(), AriError> {
        let ttl = ttl_seconds.unwrap_or(self.config.default_ttl_seconds);

        // Create metadata
        let metadata = RouteMetadata {
            destination: dst_addr,
            next_hop_address: next_hop.to_string(),
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            ttl_seconds: ttl,
        };

        // Check if route already exists
        let route_name = format!("/routing/dynamic/{}", dst_addr);
        let mut route_data = HashMap::new();
        route_data.insert(
            "next_hop_address".to_string(),
            Box::new(RibValue::String(next_hop.to_string())),
        );
        route_data.insert(
            "next_hop_rina_addr".to_string(),
            Box::new(RibValue::Integer(dst_addr as i64)),
        );

        let rib = self.rib.read().await;
        let route_exists = rib.read(&route_name).await.is_some();

        if route_exists {
            // Update existing route
            rib.update(&route_name, RibValue::Struct(route_data))
                .await
                .map_err(|e| AriError::Rib(crate::error::RibError::OperationFailed(e)))?;

            println!(
                "🔄 Updated dynamic route: {} -> {} (TTL: {}s)",
                dst_addr, next_hop, ttl
            );
        } else {
            // Create new route
            rib.create(
                route_name.clone(),
                "route".to_string(),
                RibValue::Struct(route_data),
            )
            .await
            .map_err(|e| AriError::Rib(crate::error::RibError::OperationFailed(e)))?;

            println!(
                "🛣️  Added dynamic route: {} -> {} (TTL: {}s)",
                dst_addr, next_hop, ttl
            );
        }

        // Update metadata cache
        let mut cache = self.metadata_cache.write().await;
        cache.insert(dst_addr, metadata);

        // Immediately save snapshot if persistence is enabled
        if self.config.enable_persistence {
            drop(cache); // Release lock before saving
            if let Err(e) = self.save_snapshot().await {
                eprintln!("⚠️  Failed to save snapshot after adding route: {}", e);
            } else {
                println!("  ✓ Snapshot saved immediately");
            }
        }

        Ok(())
    }

    /// Remove a dynamic route (e.g., on disconnection or expiration)
    pub async fn remove_dynamic_route(&self, dst_addr: u64) -> Result<(), AriError> {
        let route_name = format!("/routing/dynamic/{}", dst_addr);

        let rib = self.rib.read().await;
        rib.delete(&route_name)
            .await
            .map_err(|e| AriError::Rib(crate::error::RibError::OperationFailed(e)))?;

        let mut cache = self.metadata_cache.write().await;
        cache.remove(&dst_addr);

        println!("🗑️  Removed dynamic route: {}", dst_addr);

        Ok(())
    }

    /// Load routes from snapshot file (called on startup)
    pub async fn load_snapshot(&self) -> Result<usize, AriError> {
        if !self.config.enable_persistence {
            return Ok(0);
        }

        if !self.config.snapshot_path.exists() {
            println!(
                "📂 No route snapshot found at {:?}",
                self.config.snapshot_path
            );
            return Ok(0);
        }

        let snapshot = RouteSnapshot::load_from_file(&self.config.snapshot_path)?;
        let valid_routes = snapshot.filter_valid();

        let mut loaded_count = 0;
        for metadata in valid_routes {
            let next_hop: SocketAddr = metadata.next_hop_address.parse().map_err(|e| {
                AriError::Rmt(crate::error::RmtError::Network(format!(
                    "Invalid socket address in snapshot: {}",
                    e
                )))
            })?;

            let remaining_ttl = metadata.remaining_ttl();
            if remaining_ttl > 0 {
                self.add_dynamic_route(metadata.destination, next_hop, Some(remaining_ttl))
                    .await?;
                loaded_count += 1;
            }
        }

        println!(
            "✅ Loaded {} valid dynamic routes from snapshot (filtered {} expired)",
            loaded_count,
            snapshot.routes.len() - loaded_count
        );

        Ok(loaded_count)
    }

    /// Save current dynamic routes to snapshot file
    pub async fn save_snapshot(&self) -> Result<(), AriError> {
        if !self.config.enable_persistence {
            return Ok(());
        }

        let cache = self.metadata_cache.read().await;
        let routes: Vec<RouteMetadata> = cache.values().cloned().collect();
        let route_count = routes.len();

        if route_count == 0 {
            println!("ℹ️  No dynamic routes to save (cache is empty)");
            return Ok(());
        }

        let snapshot = RouteSnapshot::new(routes);
        snapshot.save_to_file(&self.config.snapshot_path)?;

        println!(
            "💾 Saved {} dynamic routes to snapshot: {:?}",
            snapshot.routes.len(),
            self.config.snapshot_path
        );

        Ok(())
    }

    /// Start background task for periodic snapshots
    pub fn start_snapshot_task(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        let resolver = self.clone();
        tokio::spawn(async move {
            if !resolver.config.enable_persistence {
                println!("⚠️  Route persistence disabled - snapshot task not started");
                return;
            }

            if resolver.config.snapshot_interval_seconds == 0 {
                println!("⚠️  Snapshot interval is 0 - snapshot task not started");
                return;
            }

            println!(
                "✅ Starting route snapshot task (interval: {}s, path: {:?})",
                resolver.config.snapshot_interval_seconds, resolver.config.snapshot_path
            );

            let mut ticker = interval(Duration::from_secs(
                resolver.config.snapshot_interval_seconds,
            ));

            loop {
                ticker.tick().await;

                // Log before attempting save
                let stats = resolver.get_stats().await;
                println!(
                    "🔄 Snapshot task tick: {} dynamic routes",
                    stats.total_dynamic_routes
                );

                if let Err(e) = resolver.save_snapshot().await {
                    eprintln!("⚠️  Failed to save route snapshot: {}", e);
                }
            }
        })
    }

    /// Get statistics about current routes
    pub async fn get_stats(&self) -> RouteStats {
        let cache = self.metadata_cache.read().await;

        let total_dynamic = cache.len();
        let expired = cache.values().filter(|m| m.is_expired()).count();

        RouteStats {
            total_dynamic_routes: total_dynamic,
            expired_routes: expired,
            valid_routes: total_dynamic - expired,
        }
    }
}

/// Statistics about route state
#[derive(Debug, Clone)]
pub struct RouteStats {
    pub total_dynamic_routes: usize,
    pub expired_routes: usize,
    pub valid_routes: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_route_metadata_expiration() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Route that never expires
        let route1 = RouteMetadata {
            destination: 100,
            next_hop_address: "127.0.0.1:8000".to_string(),
            created_at: now,
            ttl_seconds: 0,
        };
        assert!(!route1.is_expired());

        // Route created 5 seconds ago with 10 second TTL (not expired)
        let route2 = RouteMetadata {
            destination: 200,
            next_hop_address: "127.0.0.1:8001".to_string(),
            created_at: now - 5,
            ttl_seconds: 10,
        };
        assert!(!route2.is_expired());

        // Route created 15 seconds ago with 10 second TTL (expired)
        let route3 = RouteMetadata {
            destination: 300,
            next_hop_address: "127.0.0.1:8002".to_string(),
            created_at: now - 15,
            ttl_seconds: 10,
        };
        assert!(route3.is_expired());
    }

    #[test]
    fn test_route_snapshot_serialization() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let routes = vec![
            RouteMetadata {
                destination: 100,
                next_hop_address: "192.168.1.1:7000".to_string(),
                created_at: now,
                ttl_seconds: 3600,
            },
            RouteMetadata {
                destination: 200,
                next_hop_address: "192.168.1.2:7000".to_string(),
                created_at: now,
                ttl_seconds: 0,
            },
        ];

        let snapshot = RouteSnapshot::new(routes);
        let toml_str = toml::to_string_pretty(&snapshot).unwrap();

        // Verify it can be deserialized
        let parsed: RouteSnapshot = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.routes.len(), 2);
        assert_eq!(parsed.version, 1);
    }

    #[test]
    fn test_snapshot_filter_valid() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let routes = vec![
            // Valid route
            RouteMetadata {
                destination: 100,
                next_hop_address: "127.0.0.1:8000".to_string(),
                created_at: now - 5,
                ttl_seconds: 10,
            },
            // Expired route
            RouteMetadata {
                destination: 200,
                next_hop_address: "127.0.0.1:8001".to_string(),
                created_at: now - 20,
                ttl_seconds: 10,
            },
            // Never expires
            RouteMetadata {
                destination: 300,
                next_hop_address: "127.0.0.1:8002".to_string(),
                created_at: now - 100,
                ttl_seconds: 0,
            },
        ];

        let snapshot = RouteSnapshot::new(routes);
        let valid = snapshot.filter_valid();

        // Should have 2 valid routes (100 and 300)
        assert_eq!(valid.len(), 2);
        assert!(valid.iter().any(|r| r.destination == 100));
        assert!(valid.iter().any(|r| r.destination == 300));
    }
}
