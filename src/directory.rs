// SPDX-License-Identifier: EUPL-1.2-or-later
// Copyright © 2026-present ARI Contributors

//! Directory Service
//!
//! Provides name resolution and registration for RINA.
//! Maps application names to IPCP addresses.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// A naming entry in the directory
#[derive(Debug, Clone)]
pub struct DirectoryEntry {
    /// Application or process name
    pub name: String,
    /// List of addresses where this name is registered
    pub addresses: Vec<u64>,
    /// Timestamp of registration (Unix epoch seconds)
    pub timestamp: u64,
}

/// Directory Service for name resolution
#[derive(Debug, Clone)]
pub struct Directory {
    /// Map of names to directory entries
    entries: Arc<RwLock<HashMap<String, DirectoryEntry>>>,
}

impl Directory {
    /// Creates a new directory service
    pub fn new() -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Registers a name at a specific address
    pub fn register(&self, name: String, address: u64) -> Result<(), String> {
        let mut entries = self.entries.write().unwrap();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        entries
            .entry(name.clone())
            .and_modify(|e| {
                if !e.addresses.contains(&address) {
                    e.addresses.push(address);
                }
                e.timestamp = timestamp;
            })
            .or_insert(DirectoryEntry {
                name,
                addresses: vec![address],
                timestamp,
            });

        Ok(())
    }

    /// Unregisters a name from a specific address
    pub fn unregister(&self, name: &str, address: u64) -> Result<(), String> {
        let mut entries = self.entries.write().unwrap();

        if let Some(entry) = entries.get_mut(name) {
            entry.addresses.retain(|&addr| addr != address);
            if entry.addresses.is_empty() {
                entries.remove(name);
            }
            Ok(())
        } else {
            Err(format!("Name '{}' not found", name))
        }
    }

    /// Resolves a name to a list of addresses
    pub fn resolve(&self, name: &str) -> Option<Vec<u64>> {
        let entries = self.entries.read().unwrap();
        entries.get(name).map(|e| e.addresses.clone())
    }

    /// Lists all registered names
    pub fn list_names(&self) -> Vec<String> {
        let entries = self.entries.read().unwrap();
        entries.keys().cloned().collect()
    }

    /// Returns the number of registered names
    pub fn count(&self) -> usize {
        let entries = self.entries.read().unwrap();
        entries.len()
    }

    /// Clears all entries
    pub fn clear(&self) {
        let mut entries = self.entries.write().unwrap();
        entries.clear();
    }
}

impl Default for Directory {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_directory_register_and_resolve() {
        let dir = Directory::new();

        dir.register("app.example".to_string(), 1000).unwrap();

        let addresses = dir.resolve("app.example");
        assert!(addresses.is_some());
        assert_eq!(addresses.unwrap(), vec![1000]);
    }

    #[test]
    fn test_directory_multiple_addresses() {
        let dir = Directory::new();

        dir.register("service.example".to_string(), 1000).unwrap();
        dir.register("service.example".to_string(), 2000).unwrap();

        let addresses = dir.resolve("service.example").unwrap();
        assert_eq!(addresses.len(), 2);
        assert!(addresses.contains(&1000));
        assert!(addresses.contains(&2000));
    }

    #[test]
    fn test_directory_unregister() {
        let dir = Directory::new();

        dir.register("app".to_string(), 1000).unwrap();
        dir.register("app".to_string(), 2000).unwrap();

        dir.unregister("app", 1000).unwrap();

        let addresses = dir.resolve("app").unwrap();
        assert_eq!(addresses, vec![2000]);
    }

    #[test]
    fn test_directory_unregister_last_address() {
        let dir = Directory::new();

        dir.register("app".to_string(), 1000).unwrap();
        dir.unregister("app", 1000).unwrap();

        assert!(dir.resolve("app").is_none());
    }

    #[test]
    fn test_directory_list_names() {
        let dir = Directory::new();

        dir.register("app1".to_string(), 1000).unwrap();
        dir.register("app2".to_string(), 2000).unwrap();

        let names = dir.list_names();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"app1".to_string()));
        assert!(names.contains(&"app2".to_string()));
    }
}

/// Address pool for dynamic address assignment (used by bootstrap IPCP)
#[derive(Debug, Clone)]
pub struct AddressPool {
    /// Range start (inclusive)
    start: u64,
    /// Range end (inclusive)
    end: u64,
    /// Currently assigned addresses
    assigned: Arc<RwLock<std::collections::HashSet<u64>>>,
}

impl AddressPool {
    /// Creates a new address pool with the given range
    pub fn new(start: u64, end: u64) -> Self {
        Self {
            start,
            end,
            assigned: Arc::new(RwLock::new(std::collections::HashSet::new())),
        }
    }

    /// Allocates the next available address
    ///
    /// # Returns
    /// * `Ok(u64)` with the allocated address
    /// * `Err(String)` if no addresses are available
    pub fn allocate(&self) -> Result<u64, String> {
        let mut assigned = self.assigned.write().unwrap();

        // Find first available address
        for addr in self.start..=self.end {
            if !assigned.contains(&addr) {
                assigned.insert(addr);
                return Ok(addr);
            }
        }

        Err("Address pool exhausted".to_string())
    }

    /// Releases an address back to the pool
    pub fn release(&self, address: u64) -> Result<(), String> {
        let mut assigned = self.assigned.write().unwrap();

        if address < self.start || address > self.end {
            return Err("Address out of pool range".to_string());
        }

        if !assigned.remove(&address) {
            return Err("Address was not allocated".to_string());
        }

        Ok(())
    }

    /// Checks if an address is currently allocated
    pub fn is_allocated(&self, address: u64) -> bool {
        let assigned = self.assigned.read().unwrap();
        assigned.contains(&address)
    }

    /// Returns the number of allocated addresses
    pub fn allocated_count(&self) -> usize {
        let assigned = self.assigned.read().unwrap();
        assigned.len()
    }

    /// Returns the total capacity of the pool
    pub fn capacity(&self) -> u64 {
        self.end - self.start + 1
    }

    /// Returns available addresses count
    pub fn available_count(&self) -> u64 {
        self.capacity() - self.allocated_count() as u64
    }
}

#[cfg(test)]
mod address_pool_tests {
    use super::*;

    #[test]
    fn test_address_pool_allocation() {
        let pool = AddressPool::new(1000, 1005);

        let addr1 = pool.allocate().unwrap();
        let addr2 = pool.allocate().unwrap();

        assert_ne!(addr1, addr2);
        assert!(addr1 >= 1000 && addr1 <= 1005);
        assert!(addr2 >= 1000 && addr2 <= 1005);
    }

    #[test]
    fn test_address_pool_exhaustion() {
        let pool = AddressPool::new(1000, 1002); // Only 3 addresses

        let _addr1 = pool.allocate().unwrap();
        let _addr2 = pool.allocate().unwrap();
        let _addr3 = pool.allocate().unwrap();

        // Fourth allocation should fail
        assert!(pool.allocate().is_err());
    }

    #[test]
    fn test_address_pool_release() {
        let pool = AddressPool::new(1000, 1002);

        let addr = pool.allocate().unwrap();
        assert_eq!(pool.allocated_count(), 1);

        pool.release(addr).unwrap();
        assert_eq!(pool.allocated_count(), 0);

        // Should be able to allocate again
        let addr2 = pool.allocate().unwrap();
        assert_eq!(addr, addr2);
    }

    #[test]
    fn test_address_pool_capacity() {
        let pool = AddressPool::new(1000, 1010);

        assert_eq!(pool.capacity(), 11);
        assert_eq!(pool.available_count(), 11);

        pool.allocate().unwrap();
        assert_eq!(pool.available_count(), 10);
    }
}
