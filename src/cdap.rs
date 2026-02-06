// SPDX-License-Identifier: EUPL-1.2-or-later
// Copyright © 2026-present ARI Contributors

//! Common Distributed Application Protocol (CDAP)
//!
//! CDAP is used for distributed object management across IPCPs in a DIF.
//! It enables RIB synchronization and provides operations for managing
//! distributed state: CREATE, DELETE, READ, WRITE, START, STOP.

use crate::rib::{Rib, RibChange, RibValue};
use serde::{Deserialize, Serialize};
use std::fmt;

/// CDAP operation types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CdapOpCode {
    /// Create a new object
    Create,
    /// Delete an existing object
    Delete,
    /// Read an object's value
    Read,
    /// Update an object's value
    Write,
    /// Start an operation
    Start,
    /// Stop an operation
    Stop,
}

impl fmt::Display for CdapOpCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CdapOpCode::Create => write!(f, "CREATE"),
            CdapOpCode::Delete => write!(f, "DELETE"),
            CdapOpCode::Read => write!(f, "READ"),
            CdapOpCode::Write => write!(f, "WRITE"),
            CdapOpCode::Start => write!(f, "START"),
            CdapOpCode::Stop => write!(f, "STOP"),
        }
    }
}

/// CDAP message for distributed operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdapMessage {
    /// Operation code
    pub op_code: CdapOpCode,
    /// Object name (path in RIB)
    pub obj_name: String,
    /// Object class
    pub obj_class: Option<String>,
    /// Object value (for CREATE/WRITE operations)
    pub obj_value: Option<RibValue>,
    /// Unique invoke ID for request/response matching
    pub invoke_id: u64,
    /// Result code (0 = success, non-zero = error)
    pub result: i32,
    /// Result reason (error message if result != 0)
    pub result_reason: Option<String>,
    /// Sync request (for incremental RIB synchronization)
    #[serde(default)]
    pub sync_request: Option<SyncRequest>,
    /// Sync response (for incremental RIB synchronization)
    #[serde(default)]
    pub sync_response: Option<SyncResponse>,
}

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changes: Option<Vec<RibChange>>,
    /// Full snapshot (if changes is None)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full_snapshot: Option<Vec<u8>>,
    /// Error message if sync failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl CdapMessage {
    /// Creates a new CDAP request message
    pub fn new_request(
        op_code: CdapOpCode,
        obj_name: String,
        obj_class: Option<String>,
        obj_value: Option<RibValue>,
        invoke_id: u64,
    ) -> Self {
        Self {
            op_code,
            obj_name,
            obj_class,
            obj_value,
            invoke_id,
            result: 0,
            result_reason: None,
            sync_request: None,
            sync_response: None,
        }
    }

    /// Creates a new CDAP response message
    pub fn new_response(invoke_id: u64, result: i32, result_reason: Option<String>) -> Self {
        Self {
            op_code: CdapOpCode::Read, // Placeholder
            obj_name: String::new(),
            obj_class: None,
            obj_value: None,
            invoke_id,
            result,
            result_reason,
            sync_request: None,
            sync_response: None,
        }
    }

    /// Creates a new sync request message
    pub fn new_sync_request(invoke_id: u64, last_known_version: u64, requester: String) -> Self {
        Self {
            op_code: CdapOpCode::Read,
            obj_name: "rib_sync".to_string(),
            obj_class: Some("sync".to_string()),
            obj_value: None,
            invoke_id,
            result: 0,
            result_reason: None,
            sync_request: Some(SyncRequest {
                last_known_version,
                requester,
            }),
            sync_response: None,
        }
    }

    /// Creates a new sync response message
    pub fn new_sync_response(
        invoke_id: u64,
        current_version: u64,
        changes: Option<Vec<RibChange>>,
        full_snapshot: Option<Vec<u8>>,
        error: Option<String>,
    ) -> Self {
        Self {
            op_code: CdapOpCode::Read,
            obj_name: "rib_sync".to_string(),
            obj_class: Some("sync".to_string()),
            obj_value: None,
            invoke_id,
            result: if error.is_some() { 1 } else { 0 },
            result_reason: error.clone(),
            sync_request: None,
            sync_response: Some(SyncResponse {
                current_version,
                changes,
                full_snapshot,
                error,
            }),
        }
    }

    /// Checks if this is a successful response
    pub fn is_success(&self) -> bool {
        self.result == 0
    }
}

/// CDAP session for managing distributed operations
#[derive(Debug)]
pub struct CdapSession {
    /// Local RIB
    rib: Rib,
    /// Next invoke ID for outgoing requests
    next_invoke_id: u64,
}

impl CdapSession {
    /// Creates a new CDAP session with the given RIB
    pub fn new(rib: Rib) -> Self {
        Self {
            rib,
            next_invoke_id: 1,
        }
    }

    /// Generates the next invoke ID
    fn next_invoke_id(&mut self) -> u64 {
        let id = self.next_invoke_id;
        self.next_invoke_id += 1;
        id
    }

    /// Creates a CREATE request message
    pub fn create_request(
        &mut self,
        obj_name: String,
        obj_class: String,
        obj_value: RibValue,
    ) -> CdapMessage {
        CdapMessage::new_request(
            CdapOpCode::Create,
            obj_name,
            Some(obj_class),
            Some(obj_value),
            self.next_invoke_id(),
        )
    }

    /// Creates a READ request message
    pub fn read_request(&mut self, obj_name: String) -> CdapMessage {
        CdapMessage::new_request(
            CdapOpCode::Read,
            obj_name,
            None,
            None,
            self.next_invoke_id(),
        )
    }

    /// Creates a WRITE request message
    pub fn write_request(&mut self, obj_name: String, obj_value: RibValue) -> CdapMessage {
        CdapMessage::new_request(
            CdapOpCode::Write,
            obj_name,
            None,
            Some(obj_value),
            self.next_invoke_id(),
        )
    }

    /// Creates a DELETE request message
    pub fn delete_request(&mut self, obj_name: String) -> CdapMessage {
        CdapMessage::new_request(
            CdapOpCode::Delete,
            obj_name,
            None,
            None,
            self.next_invoke_id(),
        )
    }

    /// Creates a START request message (for operations like enrollment)
    pub fn start_request(&mut self, obj_name: String, obj_value: Option<RibValue>) -> CdapMessage {
        CdapMessage::new_request(
            CdapOpCode::Start,
            obj_name,
            None,
            obj_value,
            self.next_invoke_id(),
        )
    }

    /// Processes an incoming CDAP message and returns a response
    pub async fn process_message(&self, msg: &CdapMessage) -> CdapMessage {
        match msg.op_code {
            CdapOpCode::Create => self.handle_create(msg).await,
            CdapOpCode::Read => self.handle_read(msg).await,
            CdapOpCode::Write => self.handle_write(msg).await,
            CdapOpCode::Delete => self.handle_delete(msg).await,
            CdapOpCode::Start | CdapOpCode::Stop => {
                // TODO: Implement START/STOP operations
                CdapMessage::new_response(
                    msg.invoke_id,
                    -1,
                    Some("Operation not yet implemented".to_string()),
                )
            }
        }
    }

    async fn handle_create(&self, msg: &CdapMessage) -> CdapMessage {
        if msg.obj_class.is_none() || msg.obj_value.is_none() {
            return CdapMessage::new_response(
                msg.invoke_id,
                -1,
                Some("Missing class or value for CREATE".to_string()),
            );
        }

        match self
            .rib
            .create(
                msg.obj_name.clone(),
                msg.obj_class.clone().unwrap(),
                msg.obj_value.clone().unwrap(),
            )
            .await
        {
            Ok(_) => CdapMessage::new_response(msg.invoke_id, 0, None),
            Err(e) => CdapMessage::new_response(msg.invoke_id, -1, Some(e)),
        }
    }

    async fn handle_read(&self, msg: &CdapMessage) -> CdapMessage {
        match self.rib.read(&msg.obj_name).await {
            Some(obj) => {
                let mut response = CdapMessage::new_response(msg.invoke_id, 0, None);
                response.obj_value = Some(obj.value);
                response.obj_class = Some(obj.class);
                response
            }
            None => CdapMessage::new_response(
                msg.invoke_id,
                -1,
                Some(format!("Object '{}' not found", msg.obj_name)),
            ),
        }
    }

    async fn handle_write(&self, msg: &CdapMessage) -> CdapMessage {
        if msg.obj_value.is_none() {
            return CdapMessage::new_response(
                msg.invoke_id,
                -1,
                Some("Missing value for WRITE".to_string()),
            );
        }

        match self
            .rib
            .update(&msg.obj_name, msg.obj_value.clone().unwrap())
            .await
        {
            Ok(_) => CdapMessage::new_response(msg.invoke_id, 0, None),
            Err(e) => CdapMessage::new_response(msg.invoke_id, -1, Some(e)),
        }
    }

    async fn handle_delete(&self, msg: &CdapMessage) -> CdapMessage {
        match self.rib.delete(&msg.obj_name).await {
            Ok(_) => CdapMessage::new_response(msg.invoke_id, 0, None),
            Err(e) => CdapMessage::new_response(msg.invoke_id, -1, Some(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cdap_opcode_display() {
        assert_eq!(CdapOpCode::Create.to_string(), "CREATE");
        assert_eq!(CdapOpCode::Read.to_string(), "READ");
    }

    #[test]
    fn test_cdap_create_request() {
        let rib = Rib::new();
        let mut session = CdapSession::new(rib);

        let msg = session.create_request(
            "test/obj".to_string(),
            "test".to_string(),
            RibValue::Integer(42),
        );

        assert_eq!(msg.op_code, CdapOpCode::Create);
        assert_eq!(msg.obj_name, "test/obj");
        assert_eq!(msg.invoke_id, 1);
    }

    #[tokio::test]
    async fn test_cdap_session_create_and_read() {
        let rib = Rib::new();
        let mut session = CdapSession::new(rib);

        // Create a CREATE request
        let create_msg = session.create_request(
            "test/data".to_string(),
            "data".to_string(),
            RibValue::String("hello".to_string()),
        );

        // Process the CREATE request
        let create_response = session.process_message(&create_msg).await;
        assert!(create_response.is_success());

        // Create a READ request
        let read_msg = session.read_request("test/data".to_string());

        // Process the READ request
        let read_response = session.process_message(&read_msg).await;
        assert!(read_response.is_success());
        assert_eq!(read_response.obj_value.unwrap().as_string(), Some("hello"));
    }

    #[tokio::test]
    async fn test_cdap_write_operation() {
        let rib = Rib::new();
        let mut session = CdapSession::new(rib);

        // First create an object
        let create_msg = session.create_request(
            "counter".to_string(),
            "config".to_string(),
            RibValue::Integer(0),
        );
        session.process_message(&create_msg).await;

        // Update the object
        let write_msg = session.write_request("counter".to_string(), RibValue::Integer(10));
        let write_response = session.process_message(&write_msg).await;
        assert!(write_response.is_success());

        // Verify the update
        let read_msg = session.read_request("counter".to_string());
        let read_response = session.process_message(&read_msg).await;
        assert_eq!(read_response.obj_value.unwrap().as_integer(), Some(10));
    }

    #[tokio::test]
    async fn test_cdap_delete_operation() {
        let rib = Rib::new();
        let mut session = CdapSession::new(rib);

        // Create an object
        let create_msg = session.create_request(
            "temp".to_string(),
            "temp".to_string(),
            RibValue::Boolean(true),
        );
        session.process_message(&create_msg).await;

        // Delete the object
        let delete_msg = session.delete_request("temp".to_string());
        let delete_response = session.process_message(&delete_msg).await;
        assert!(delete_response.is_success());

        // Verify it's gone
        let read_msg = session.read_request("temp".to_string());
        let read_response = session.process_message(&read_msg).await;
        assert!(!read_response.is_success());
    }

    #[test]
    fn test_invoke_id_increment() {
        let rib = Rib::new();
        let mut session = CdapSession::new(rib);

        let msg1 = session.read_request("obj1".to_string());
        let msg2 = session.read_request("obj2".to_string());

        assert_eq!(msg1.invoke_id, 1);
        assert_eq!(msg2.invoke_id, 2);
    }
}
