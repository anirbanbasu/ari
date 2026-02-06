// SPDX-License-Identifier: EUPL-1.2-or-later
// Copyright © 2026-present ARI Contributors

//! Resource Information Base (RIB)
//!
//! The RIB is a central component in RINA that stores and manages all information
//! about the IPC Process state, including:
//! - Directory (name-to-address mappings)
//! - Flow state
//! - Neighbor information
//! - Routing information
//! - QoS/policy configurations
//!
//! The RIB is distributed across all IPCPs in a DIF and kept consistent through CDAP.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

/// Represents an object stored in the RIB with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RibObject {
    /// Unique identifier for this object
    pub name: String,
    /// Object class (e.g., "flow", "neighbor", "address")
    pub class: String,
    /// The actual data payload
    pub value: RibValue,
    /// Version counter for consistency tracking
    pub version: u64,
    /// Last modification timestamp (Unix epoch)
    pub last_modified: u64,
}

/// Represents different types of values that can be stored in the RIB
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RibValue {
    String(String),
    Integer(i64),
    Boolean(bool),
    Bytes(Vec<u8>),
    Struct(HashMap<String, Box<RibValue>>),
}

impl RibValue {
    /// Attempts to extract a string value
    pub fn as_string(&self) -> Option<&str> {
        match self {
            RibValue::String(s) => Some(s),
            _ => None,
        }
    }

    /// Attempts to extract an integer value
    pub fn as_integer(&self) -> Option<i64> {
        match self {
            RibValue::Integer(i) => Some(*i),
            _ => None,
        }
    }

    /// Attempts to extract a boolean value
    pub fn as_boolean(&self) -> Option<bool> {
        match self {
            RibValue::Boolean(b) => Some(*b),
            _ => None,
        }
    }
}

/// The Resource Information Base
///
/// Thread-safe storage for all IPC Process state information.
/// Uses RwLock for concurrent read access while maintaining write consistency.
#[derive(Debug, Clone)]
pub struct Rib {
    /// Internal storage of RIB objects, keyed by object name
    objects: Arc<RwLock<HashMap<String, RibObject>>>,
    /// Counter for generating object versions
    version_counter: Arc<RwLock<u64>>,
}

impl Rib {
    /// Creates a new, empty RIB
    pub fn new() -> Self {
        Self {
            objects: Arc::new(RwLock::new(HashMap::new())),
            version_counter: Arc::new(RwLock::new(0)),
        }
    }

    /// Creates a RIB object with the given name, class, and value
    ///
    /// # Arguments
    /// * `name` - Unique identifier for the object
    /// * `class` - Object class/type
    /// * `value` - The value to store
    ///
    /// # Returns
    /// * `Ok(())` if the object was created successfully
    /// * `Err(String)` if an object with that name already exists
    pub async fn create(&self, name: String, class: String, value: RibValue) -> Result<(), String> {
        let mut objects = self.objects.write().await;

        if objects.contains_key(&name) {
            return Err(format!("Object '{}' already exists", name));
        }

        let version = self.next_version().await;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let obj = RibObject {
            name: name.clone(),
            class,
            value,
            version,
            last_modified: now,
        };

        objects.insert(name, obj);
        Ok(())
    }

    /// Reads a RIB object by name
    ///
    /// # Arguments
    /// * `name` - The name of the object to retrieve
    ///
    /// # Returns
    /// * `Some(RibObject)` if found
    /// * `None` if not found
    pub async fn read(&self, name: &str) -> Option<RibObject> {
        let objects = self.objects.read().await;
        objects.get(name).cloned()
    }

    /// Updates an existing RIB object
    ///
    /// # Arguments
    /// * `name` - The name of the object to update
    /// * `value` - The new value
    ///
    /// # Returns
    /// * `Ok(())` if updated successfully
    /// * `Err(String)` if the object doesn't exist
    pub async fn update(&self, name: &str, value: RibValue) -> Result<(), String> {
        let mut objects = self.objects.write().await;

        match objects.get_mut(name) {
            Some(obj) => {
                obj.value = value;
                obj.version = self.next_version().await;
                obj.last_modified = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                Ok(())
            }
            None => Err(format!("Object '{}' not found", name)),
        }
    }

    /// Deletes a RIB object by name
    ///
    /// # Arguments
    /// * `name` - The name of the object to delete
    ///
    /// # Returns
    /// * `Ok(())` if deleted successfully
    /// * `Err(String)` if the object doesn't exist
    pub async fn delete(&self, name: &str) -> Result<(), String> {
        let mut objects = self.objects.write().await;

        match objects.remove(name) {
            Some(_) => Ok(()),
            None => Err(format!("Object '{}' not found", name)),
        }
    }

    /// Lists all objects of a given class
    ///
    /// # Arguments
    /// * `class` - The object class to filter by
    ///
    /// # Returns
    /// A vector of object names matching the class
    pub async fn list_by_class(&self, class: &str) -> Vec<String> {
        let objects = self.objects.read().await;
        objects
            .values()
            .filter(|obj| obj.class == class)
            .map(|obj| obj.name.clone())
            .collect()
    }

    /// Lists all object names in the RIB
    pub async fn list_all(&self) -> Vec<String> {
        let objects = self.objects.read().await;
        objects.keys().cloned().collect()
    }

    /// Returns the total number of objects in the RIB
    pub async fn count(&self) -> usize {
        let objects = self.objects.read().await;
        objects.len()
    }

    /// Clears all objects from the RIB
    pub async fn clear(&self) {
        let mut objects = self.objects.write().await;
        objects.clear();
    }

    /// Serializes the entire RIB into a byte vector for synchronization
    ///
    /// Uses bincode for efficient binary serialization
    ///
    /// # Returns
    /// A serialized representation of all RIB objects
    pub async fn serialize(&self) -> Vec<u8> {
        let objects = self.objects.read().await;

        // Collect all objects into a vector
        let all_objects: Vec<RibObject> = objects.values().cloned().collect();

        // Serialize using bincode
        bincode::serialize(&all_objects).unwrap_or_else(|e| {
            eprintln!("Failed to serialize RIB: {}", e);
            Vec::new()
        })
    }

    /// Deserializes a RIB snapshot and merges it into this RIB
    ///
    /// Uses bincode for deserialization
    ///
    /// # Arguments
    /// * `data` - Serialized RIB data
    ///
    /// # Returns
    /// * `Ok(usize)` with the number of objects synchronized
    /// * `Err(String)` if deserialization fails
    pub async fn deserialize(&self, data: &[u8]) -> Result<usize, String> {
        if data.is_empty() {
            return Ok(0);
        }

        // Deserialize using bincode
        let objects: Vec<RibObject> =
            bincode::deserialize(data).map_err(|e| format!("Failed to deserialize RIB: {}", e))?;

        // Merge objects into RIB
        let count = self.merge_objects(objects).await;
        Ok(count)
    }

    /// Gets all objects from the RIB (for synchronization)
    pub async fn get_all_objects(&self) -> Vec<RibObject> {
        let objects = self.objects.read().await;
        objects.values().cloned().collect()
    }

    /// Merges objects from another RIB, using version numbers to resolve conflicts
    ///
    /// # Arguments
    /// * `objects` - Objects to merge into this RIB
    ///
    /// # Returns
    /// The number of objects updated or created
    pub async fn merge_objects(&self, objects: Vec<RibObject>) -> usize {
        let mut local_objects = self.objects.write().await;
        let mut merged_count = 0;

        for obj in objects {
            match local_objects.get(&obj.name) {
                Some(existing) => {
                    // Only update if incoming version is newer
                    if obj.version > existing.version {
                        local_objects.insert(obj.name.clone(), obj);
                        merged_count += 1;
                    }
                }
                None => {
                    // New object, add it
                    local_objects.insert(obj.name.clone(), obj);
                    merged_count += 1;
                }
            }
        }

        merged_count
    }

    /// Generates the next version number
    async fn next_version(&self) -> u64 {
        let mut counter = self.version_counter.write().await;
        *counter += 1;
        *counter
    }
}

impl Default for Rib {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rib_create_and_read() {
        let rib = Rib::new();

        let result = rib
            .create(
                "test-object".to_string(),
                "test-class".to_string(),
                RibValue::String("test-value".to_string()),
            )
            .await;

        assert!(result.is_ok());

        let obj = rib.read("test-object").await;
        assert!(obj.is_some());

        let obj = obj.unwrap();
        assert_eq!(obj.name, "test-object");
        assert_eq!(obj.class, "test-class");
        assert_eq!(obj.value.as_string(), Some("test-value"));
    }

    #[tokio::test]
    async fn test_rib_update() {
        let rib = Rib::new();

        rib.create(
            "test".to_string(),
            "class".to_string(),
            RibValue::Integer(42),
        )
        .await
        .unwrap();

        let result = rib.update("test", RibValue::Integer(100)).await;
        assert!(result.is_ok());

        let obj = rib.read("test").await.unwrap();
        assert_eq!(obj.value.as_integer(), Some(100));
    }

    #[tokio::test]
    async fn test_rib_delete() {
        let rib = Rib::new();

        rib.create(
            "test".to_string(),
            "class".to_string(),
            RibValue::Boolean(true),
        )
        .await
        .unwrap();

        assert!(rib.delete("test").await.is_ok());
        assert!(rib.read("test").await.is_none());
    }

    #[tokio::test]
    async fn test_rib_list_by_class() {
        let rib = Rib::new();

        rib.create(
            "obj1".to_string(),
            "type-a".to_string(),
            RibValue::Integer(1),
        )
        .await
        .unwrap();
        rib.create(
            "obj2".to_string(),
            "type-b".to_string(),
            RibValue::Integer(2),
        )
        .await
        .unwrap();
        rib.create(
            "obj3".to_string(),
            "type-a".to_string(),
            RibValue::Integer(3),
        )
        .await
        .unwrap();

        let type_a_objects = rib.list_by_class("type-a").await;
        assert_eq!(type_a_objects.len(), 2);
        assert!(type_a_objects.contains(&"obj1".to_string()));
        assert!(type_a_objects.contains(&"obj3".to_string()));
    }

    #[tokio::test]
    async fn test_rib_duplicate_create() {
        let rib = Rib::new();

        rib.create("dup".to_string(), "class".to_string(), RibValue::Integer(1))
            .await
            .unwrap();
        let result = rib
            .create("dup".to_string(), "class".to_string(), RibValue::Integer(2))
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_rib_serialization_roundtrip() {
        let rib = Rib::new();

        // Add various types of objects
        rib.create(
            "string-obj".to_string(),
            "test".to_string(),
            RibValue::String("hello world".to_string()),
        )
        .await
        .unwrap();

        rib.create(
            "int-obj".to_string(),
            "test".to_string(),
            RibValue::Integer(42),
        )
        .await
        .unwrap();

        rib.create(
            "bool-obj".to_string(),
            "test".to_string(),
            RibValue::Boolean(true),
        )
        .await
        .unwrap();

        rib.create(
            "bytes-obj".to_string(),
            "test".to_string(),
            RibValue::Bytes(vec![1, 2, 3, 4, 5]),
        )
        .await
        .unwrap();

        // Create a nested struct
        let mut struct_map = HashMap::new();
        struct_map.insert(
            "field1".to_string(),
            Box::new(RibValue::String("value1".to_string())),
        );
        struct_map.insert("field2".to_string(), Box::new(RibValue::Integer(100)));

        rib.create(
            "struct-obj".to_string(),
            "complex".to_string(),
            RibValue::Struct(struct_map),
        )
        .await
        .unwrap();

        // Serialize the RIB
        let serialized = rib.serialize().await;
        assert!(!serialized.is_empty());

        // Create a new RIB and deserialize
        let rib2 = Rib::new();
        let count = rib2.deserialize(&serialized).await.unwrap();
        assert_eq!(count, 5);

        // Verify all objects match
        let obj1 = rib2.read("string-obj").await.unwrap();
        assert_eq!(obj1.value.as_string(), Some("hello world"));
        assert_eq!(obj1.class, "test");

        let obj2 = rib2.read("int-obj").await.unwrap();
        assert_eq!(obj2.value.as_integer(), Some(42));

        let obj3 = rib2.read("bool-obj").await.unwrap();
        assert_eq!(obj3.value.as_boolean(), Some(true));

        let obj4 = rib2.read("bytes-obj").await.unwrap();
        if let RibValue::Bytes(b) = &obj4.value {
            assert_eq!(b, &vec![1, 2, 3, 4, 5]);
        } else {
            panic!("Expected Bytes value");
        }

        let obj5 = rib2.read("struct-obj").await.unwrap();
        assert_eq!(obj5.class, "complex");
    }

    #[tokio::test]
    async fn test_rib_empty_serialization() {
        let rib = Rib::new();

        // Serialize empty RIB
        let serialized = rib.serialize().await;
        assert!(!serialized.is_empty());

        // Deserialize into another RIB
        let rib2 = Rib::new();
        let count = rib2.deserialize(&serialized).await.unwrap();
        assert_eq!(count, 0);
        assert_eq!(rib2.count().await, 0);
    }

    #[tokio::test]
    async fn test_rib_deserialize_empty_data() {
        let rib = Rib::new();

        // Deserializing empty data should succeed with 0 count
        let count = rib.deserialize(&[]).await.unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_rib_version_preservation() {
        let rib = Rib::new();

        // Create objects
        rib.create("obj1".to_string(), "test".to_string(), RibValue::Integer(1))
            .await
            .unwrap();

        // Get original version
        let original = rib.read("obj1").await.unwrap();
        let original_version = original.version;
        let original_modified = original.last_modified;

        // Serialize and deserialize
        let serialized = rib.serialize().await;
        let rib2 = Rib::new();
        rib2.deserialize(&serialized).await.unwrap();

        // Verify version and timestamp preserved
        let restored = rib2.read("obj1").await.unwrap();
        assert_eq!(restored.version, original_version);
        assert_eq!(restored.last_modified, original_modified);
    }

    #[tokio::test]
    async fn test_rib_merge_version_conflict() {
        let rib = Rib::new();

        // Create an object with version 1
        rib.create(
            "obj1".to_string(),
            "test".to_string(),
            RibValue::Integer(100),
        )
        .await
        .unwrap();

        let obj_v1 = rib.read("obj1").await.unwrap();
        assert_eq!(obj_v1.version, 1);

        // Update to create version 2
        rib.update("obj1", RibValue::Integer(200)).await.unwrap();
        let obj_v2 = rib.read("obj1").await.unwrap();
        assert_eq!(obj_v2.version, 2);
        assert_eq!(obj_v2.value.as_integer(), Some(200));

        // Create another RIB with the old version
        let rib2 = Rib::new();
        rib2.deserialize(&bincode::serialize(&vec![obj_v1]).unwrap())
            .await
            .unwrap();

        // Merge the newer version into rib2
        let merged = rib2.merge_objects(vec![obj_v2.clone()]).await;
        assert_eq!(merged, 1);

        // Verify the newer version won
        let result = rib2.read("obj1").await.unwrap();
        assert_eq!(result.version, 2);
        assert_eq!(result.value.as_integer(), Some(200));
    }

    #[tokio::test]
    async fn test_rib_merge_ignore_older_version() {
        let rib = Rib::new();

        // Create object with version 2
        rib.create(
            "obj1".to_string(),
            "test".to_string(),
            RibValue::Integer(200),
        )
        .await
        .unwrap();
        rib.update("obj1", RibValue::Integer(200)).await.unwrap();

        let obj_v2 = rib.read("obj1").await.unwrap();
        assert_eq!(obj_v2.version, 2);

        // Try to merge an older version
        let mut old_obj = obj_v2.clone();
        old_obj.version = 1;
        old_obj.value = RibValue::Integer(100);

        let merged = rib.merge_objects(vec![old_obj]).await;
        assert_eq!(merged, 0); // Should not merge older version

        // Verify original version unchanged
        let result = rib.read("obj1").await.unwrap();
        assert_eq!(result.version, 2);
        assert_eq!(result.value.as_integer(), Some(200));
    }

    #[tokio::test]
    async fn test_rib_get_all_objects() {
        let rib = Rib::new();

        // Add multiple objects
        rib.create(
            "obj1".to_string(),
            "type-a".to_string(),
            RibValue::Integer(1),
        )
        .await
        .unwrap();
        rib.create(
            "obj2".to_string(),
            "type-b".to_string(),
            RibValue::Integer(2),
        )
        .await
        .unwrap();
        rib.create(
            "obj3".to_string(),
            "type-a".to_string(),
            RibValue::Integer(3),
        )
        .await
        .unwrap();

        // Get all objects
        let all_objects = rib.get_all_objects().await;
        assert_eq!(all_objects.len(), 3);

        // Verify all names present
        let names: Vec<String> = all_objects.iter().map(|o| o.name.clone()).collect();
        assert!(names.contains(&"obj1".to_string()));
        assert!(names.contains(&"obj2".to_string()));
        assert!(names.contains(&"obj3".to_string()));
    }
}
