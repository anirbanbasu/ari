// SPDX-License-Identifier: EUPL-1.2-or-later
// Copyright © 2026-present ARI Contributors

//! IPCP Enrollment
//!
//! Handles the enrollment process where a new IPCP joins a DIF.
//! Fully async implementation with timeout and retry logic.

use crate::cdap::{CdapMessage, CdapOpCode};
use crate::directory::AddressPool;
use crate::pdu::Pdu;
use crate::rib::{Rib, RibValue};
use crate::shim::UdpShim;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::{sleep, timeout};

/// Configuration for enrollment behavior
#[derive(Debug, Clone)]
pub struct EnrollmentConfig {
    /// Timeout for a single enrollment attempt
    pub timeout: Duration,
    /// Maximum number of retry attempts
    pub max_retries: u32,
    /// Initial backoff duration in milliseconds (doubles on each retry)
    pub initial_backoff_ms: u64,
}

impl Default for EnrollmentConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(5),
            max_retries: 3,
            initial_backoff_ms: 1000,
        }
    }
}

/// Enrollment state
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnrollmentState {
    /// Not enrolled
    NotEnrolled,
    /// Enrollment initiated
    Initiated,
    /// Authenticating
    Authenticating,
    /// Synchronizing RIB
    Synchronizing,
    /// Enrollment complete
    Enrolled,
    /// Enrollment failed
    Failed(String),
}

/// Enrollment request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrollmentRequest {
    /// IPCP name requesting enrollment
    pub ipcp_name: String,
    /// IPCP address (0 if requesting dynamic assignment)
    pub ipcp_address: u64,
    /// DIF name to join
    pub dif_name: String,
    /// Timestamp of request
    pub timestamp: u64,
    /// Whether requesting dynamic address assignment
    pub request_address: bool,
}

/// Enrollment response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrollmentResponse {
    /// Whether enrollment was accepted
    pub accepted: bool,
    /// Error message if rejected
    pub error: Option<String>,
    /// Assigned address (if requested and accepted)
    pub assigned_address: Option<u64>,
    /// DIF name
    pub dif_name: String,
    /// RIB snapshot for synchronization
    pub rib_snapshot: Option<Vec<u8>>,
}

/// DIF configuration provided during enrollment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DifConfiguration {
    /// DIF name
    pub dif_name: String,
    /// Address assignment for the new IPCP
    pub assigned_address: u64,
    /// List of neighbor IPCPs
    pub neighbors: Vec<NeighborInfo>,
    /// RIB snapshot for synchronization
    pub rib_snapshot: Vec<u8>, // Serialized RIB data
}

/// Information about a neighbor IPCP
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeighborInfo {
    /// Neighbor IPCP name
    pub name: String,
    /// Neighbor address
    pub address: u64,
    /// Whether this neighbor is currently reachable
    pub reachable: bool,
}

/// Enrollment manager - fully async implementation
#[derive(Debug)]
pub struct EnrollmentManager {
    /// Current enrollment state
    state: EnrollmentState,
    /// Local IPCP name
    ipcp_name: Option<String>,
    /// Local RINA address
    local_addr: u64,
    /// Local RIB
    rib: Rib,
    /// UDP shim for network communication
    shim: Arc<UdpShim>,
    /// Enrollment configuration
    config: EnrollmentConfig,
    /// Address pool for bootstrap IPCP (None for member IPCPs)
    address_pool: Option<Arc<AddressPool>>,
}

impl EnrollmentManager {
    /// Creates a new enrollment manager
    pub fn new(rib: Rib, shim: Arc<UdpShim>, local_addr: u64) -> Self {
        Self::with_config(rib, shim, local_addr, EnrollmentConfig::default())
    }

    /// Creates a new enrollment manager with custom configuration
    pub fn with_config(
        rib: Rib,
        shim: Arc<UdpShim>,
        local_addr: u64,
        config: EnrollmentConfig,
    ) -> Self {
        Self {
            state: EnrollmentState::NotEnrolled,
            ipcp_name: None,
            local_addr,
            rib,
            shim,
            config,
            address_pool: None,
        }
    }

    /// Creates a bootstrap enrollment manager with address pool
    pub fn new_bootstrap(
        rib: Rib,
        shim: Arc<UdpShim>,
        local_addr: u64,
        pool_start: u64,
        pool_end: u64,
    ) -> Self {
        Self {
            state: EnrollmentState::Enrolled, // Bootstrap is pre-enrolled
            ipcp_name: None,
            local_addr,
            rib,
            shim,
            config: EnrollmentConfig::default(),
            address_pool: Some(Arc::new(AddressPool::new(pool_start, pool_end))),
        }
    }

    /// Sets the IPCP name
    pub fn set_ipcp_name(&mut self, name: String) {
        self.ipcp_name = Some(name);
        self.state = EnrollmentState::Initiated;
    }

    /// Returns the current enrollment state
    pub fn state(&self) -> &EnrollmentState {
        &self.state
    }

    /// Checks if enrolled
    pub fn is_enrolled(&self) -> bool {
        self.state == EnrollmentState::Enrolled
    }

    /// Returns the local address (may be updated after enrollment)
    pub fn local_addr(&self) -> u64 {
        self.local_addr
    }

    /// Enrol with bootstrap IPCP with timeout and retry logic
    pub async fn enrol_with_bootstrap(&mut self, bootstrap_addr: u64) -> Result<String, String> {
        for attempt in 1..=self.config.max_retries {
            println!("Enrollment attempt {}/{}", attempt, self.config.max_retries);

            match timeout(self.config.timeout, self.try_enrol(bootstrap_addr)).await {
                Ok(Ok(dif_name)) => {
                    println!("Successfully enrolled in DIF: {}", dif_name);
                    return Ok(dif_name);
                }
                Ok(Err(e)) => {
                    eprintln!("Enrollment attempt {} failed: {}", attempt, e);
                }
                Err(_) => {
                    eprintln!("Enrollment attempt {} timed out", attempt);
                }
            }

            if attempt < self.config.max_retries {
                let backoff =
                    Duration::from_millis(self.config.initial_backoff_ms * (1 << (attempt - 1)));
                println!("Retrying in {:?}...", backoff);
                sleep(backoff).await;
            }
        }

        Err(format!(
            "Enrollment failed after {} attempts",
            self.config.max_retries
        ))
    }

    /// Single enrollment attempt
    async fn try_enrol(&mut self, bootstrap_addr: u64) -> Result<String, String> {
        let ipcp_name = self.ipcp_name.as_ref().ok_or("IPCP name not set")?.clone();

        // Create enrollment request
        let request = EnrollmentRequest {
            ipcp_name: ipcp_name.clone(),
            ipcp_address: self.local_addr,
            dif_name: String::new(), // Will be provided by bootstrap
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            request_address: self.local_addr == 0, // Request address if we don't have one
        };

        // Create CDAP message with enrollment request
        let cdap_msg = CdapMessage {
            op_code: CdapOpCode::Create,
            obj_name: ipcp_name.clone(),
            obj_class: Some("enrollment".to_string()),
            obj_value: Some(RibValue::Bytes(
                bincode::serialize(&request)
                    .map_err(|e| format!("Failed to serialize request: {}", e))?,
            )),
            invoke_id: 1,
            result: 0,
            result_reason: None,
        };

        // Serialize CDAP message with bincode
        let cdap_bytes = bincode::serialize(&cdap_msg)
            .map_err(|e| format!("Failed to serialize CDAP message: {}", e))?;

        // Create PDU with CDAP payload
        let pdu = Pdu::new_data(
            self.local_addr, // src_addr - member's configured address (or 0)
            bootstrap_addr,  // dst_addr
            0,               // src_cep_id
            0,               // dst_cep_id
            0,               // sequence_num
            cdap_bytes,      // payload
        );

        // Send enrollment request
        self.shim
            .send_pdu(&pdu)
            .map_err(|e| format!("Failed to send enrollment request: {}", e))?;

        println!("Sent enrollment request to bootstrap IPCP");

        // Wait for response
        let response = self.receive_response().await?;

        // Deserialize enrollment response from CDAP message
        let response_bytes = response
            .obj_value
            .as_ref()
            .ok_or("Response does not contain value")?;

        let enroll_response: EnrollmentResponse = match response_bytes {
            RibValue::Bytes(bytes) => bincode::deserialize(bytes)
                .map_err(|e| format!("Failed to deserialize enrollment response: {}", e))?,
            RibValue::String(s) => {
                // Legacy support for old string-based responses
                EnrollmentResponse {
                    accepted: true,
                    error: None,
                    assigned_address: None,
                    dif_name: s.clone(),
                    rib_snapshot: None,
                }
            }
            _ => return Err("Invalid response format".to_string()),
        };

        if !enroll_response.accepted {
            return Err(enroll_response
                .error
                .unwrap_or_else(|| "Enrollment rejected".to_string()));
        }

        // Update local address if one was assigned
        if let Some(assigned_addr) = enroll_response.assigned_address {
            println!("Received assigned address: {}", assigned_addr);
            self.local_addr = assigned_addr;

            // Store assigned address in RIB
            let _ = self
                .rib
                .create(
                    "/local/address".to_string(),
                    "address".to_string(),
                    RibValue::Integer(assigned_addr as i64),
                )
                .await;
        }

        // Synchronize RIB if snapshot provided
        if let Some(rib_data) = enroll_response.rib_snapshot {
            println!("Synchronizing RIB...");
            match self.rib.deserialize(&rib_data).await {
                Ok(count) => println!("Synchronized {} RIB objects", count),
                Err(e) => println!("Warning: Failed to sync RIB: {}", e),
            }
        }

        let dif_name = enroll_response.dif_name.clone();

        // Update state
        self.state = EnrollmentState::Enrolled;

        // Store DIF name in RIB
        let _ = self
            .rib
            .create(
                "/dif/name".to_string(),
                "dif_info".to_string(),
                RibValue::String(dif_name.clone()),
            )
            .await;

        // Request routing table from bootstrap
        println!("Requesting routing table from bootstrap...");
        let _ = self.sync_routes_from_bootstrap(bootstrap_addr).await;

        Ok(dif_name)
    }

    /// Synchronize routing table from bootstrap's RIB
    async fn sync_routes_from_bootstrap(&self, bootstrap_addr: u64) -> Result<(), String> {
        // Request all static routes from bootstrap
        let cdap_msg = CdapMessage {
            op_code: CdapOpCode::Read,
            obj_name: "/routing/static/*".to_string(),
            obj_class: Some("static_route".to_string()),
            obj_value: None,
            invoke_id: 2,
            result: 0,
            result_reason: None,
        };

        let cdap_bytes = bincode::serialize(&cdap_msg)
            .map_err(|e| format!("Failed to serialize CDAP message: {}", e))?;

        let pdu = Pdu::new_data(self.local_addr, bootstrap_addr, 0, 0, 0, cdap_bytes);

        self.shim
            .send_pdu(&pdu)
            .map_err(|e| format!("Failed to send route request: {}", e))?;

        // Wait for routing table response (no filter on obj_class)
        match self.receive_cdap_response(None).await {
            Ok(response) => {
                if let Some(RibValue::Struct(routes)) = response.obj_value {
                    println!("Received {} routes from bootstrap", routes.len());

                    // Store routes in local RIB
                    for (dest, route_info) in routes {
                        let route_name = format!("/routing/static/{}", dest);
                        let _ = self
                            .rib
                            .create(route_name, "static_route".to_string(), *route_info)
                            .await;
                    }
                }
                Ok(())
            }
            Err(e) => {
                println!("Warning: Failed to sync routes: {}", e);
                Ok(()) // Non-fatal - continue enrollment
            }
        }
    }

    /// Receive enrollment response with polling
    async fn receive_response(&self) -> Result<CdapMessage, String> {
        self.receive_cdap_response(Some("enrollment")).await
    }

    /// Receive any CDAP response with polling
    async fn receive_cdap_response(
        &self,
        expected_class: Option<&str>,
    ) -> Result<CdapMessage, String> {
        let poll_interval = Duration::from_millis(100);
        let max_polls = (self.config.timeout.as_millis() / poll_interval.as_millis()) as u32;

        for _ in 0..max_polls {
            if let Some((pdu, _src_addr)) = self
                .shim
                .receive_pdu()
                .map_err(|e| format!("Failed to receive PDU: {}", e))?
            {
                // Deserialize CDAP message from PDU payload
                let cdap_msg: CdapMessage = bincode::deserialize(&pdu.payload)
                    .map_err(|e| format!("Failed to deserialize CDAP message: {}", e))?;

                // If expected_class is specified, filter by it
                if let Some(expected) = expected_class {
                    if cdap_msg.obj_class.as_deref() == Some(expected) {
                        if cdap_msg.result == 0 {
                            return Ok(cdap_msg);
                        } else {
                            return Err(format!("Request rejected with code: {}", cdap_msg.result));
                        }
                    }
                } else {
                    // Accept any CDAP message if no filter specified
                    if cdap_msg.result == 0 {
                        return Ok(cdap_msg);
                    } else {
                        return Err(format!("Request rejected with code: {}", cdap_msg.result));
                    }
                }
            }

            sleep(poll_interval).await;
        }

        Err("No response received".to_string())
    }

    /// Handle incoming enrollment request (bootstrap side)
    pub async fn handle_enrollment_request(
        &self,
        pdu: &Pdu,
        src_socket_addr: SocketAddr,
    ) -> Result<(), String> {
        // Register the peer mapping so we can send response back
        self.shim.register_peer(pdu.src_addr, src_socket_addr);

        // Deserialize CDAP message from PDU payload
        let cdap_msg: CdapMessage = bincode::deserialize(&pdu.payload)
            .map_err(|e| format!("Failed to deserialize CDAP message: {}", e))?;

        // Check if this is an enrollment request
        if cdap_msg.obj_class.as_deref() != Some("enrollment")
            || cdap_msg.op_code != CdapOpCode::Create
        {
            return Err("Not an enrollment request".to_string());
        }

        // Extract enrollment request
        let enroll_request: EnrollmentRequest = match &cdap_msg.obj_value {
            Some(RibValue::Bytes(bytes)) => bincode::deserialize(bytes)
                .map_err(|e| format!("Failed to deserialize request: {}", e))?,
            Some(RibValue::String(name)) => {
                // Legacy support for old string-based requests
                EnrollmentRequest {
                    ipcp_name: name.clone(),
                    ipcp_address: pdu.src_addr,
                    dif_name: String::new(),
                    timestamp: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
                    request_address: false,
                }
            }
            _ => return Err("Invalid enrollment request format".to_string()),
        };

        println!(
            "Received enrollment request from: {} (requesting address: {})",
            enroll_request.ipcp_name, enroll_request.request_address
        );

        // Get DIF name from RIB
        let dif_name_obj = self
            .rib
            .read("/dif/name")
            .await
            .ok_or("Bootstrap DIF name not set in RIB")?;
        let dif_name = dif_name_obj
            .value
            .as_string()
            .ok_or("DIF name is not a string")?
            .to_string();

        // Allocate address if requested
        let assigned_address = if enroll_request.request_address {
            match &self.address_pool {
                Some(pool) => match pool.allocate() {
                    Ok(addr) => {
                        println!("  ✓ Allocated address: {}", addr);
                        Some(addr)
                    }
                    Err(e) => {
                        println!("  ✗ Failed to allocate address: {}", e);
                        // Send rejection response
                        let error_response = EnrollmentResponse {
                            accepted: false,
                            error: Some(format!("Address allocation failed: {}", e)),
                            assigned_address: None,
                            dif_name: dif_name.clone(),
                            rib_snapshot: None,
                        };
                        self.send_enroll_response(pdu, &error_response, &cdap_msg)
                            .await?;
                        return Ok(());
                    }
                },
                None => {
                    println!("  ✗ No address pool configured");
                    return Err("Bootstrap has no address pool".to_string());
                }
            }
        } else {
            None
        };

        // Get RIB snapshot for synchronization
        let rib_snapshot = Some(self.rib.serialize().await);

        // Create success response
        let response = EnrollmentResponse {
            accepted: true,
            error: None,
            assigned_address,
            dif_name: dif_name.clone(),
            rib_snapshot,
        };

        // Send response
        self.send_enroll_response(pdu, &response, &cdap_msg).await?;

        println!(
            "Sent enrollment response to {} with DIF name: {}",
            enroll_request.ipcp_name, dif_name
        );

        // Add dynamic route for the enrolled member
        let member_addr = assigned_address.unwrap_or(pdu.src_addr);
        if member_addr != 0 {
            // If we assigned a new address, update the peer mapping
            if let Some(new_addr) = assigned_address {
                self.shim.register_peer(new_addr, src_socket_addr);
                println!(
                    "  ✓ Updated peer mapping: {} → {}",
                    new_addr, src_socket_addr
                );
            }

            let route_name = format!("/routing/dynamic/{}", member_addr);

            // Check if route already exists
            if self.rib.read(&route_name).await.is_none() {
                // Route doesn't exist, create it
                let route_value = RibValue::Struct({
                    let mut map = std::collections::HashMap::new();
                    map.insert(
                        "destination".to_string(),
                        Box::new(RibValue::String(member_addr.to_string())),
                    );
                    map.insert(
                        "next_hop_address".to_string(),
                        Box::new(RibValue::String(src_socket_addr.to_string())),
                    );
                    map.insert(
                        "next_hop_rina_addr".to_string(),
                        Box::new(RibValue::String(member_addr.to_string())),
                    );
                    map
                });

                self.rib
                    .create(route_name.clone(), "route".to_string(), route_value)
                    .await
                    .map_err(|e| format!("Failed to create dynamic route: {}", e))?;

                println!(
                    "  ✓ Created dynamic route: {} → {} ({})",
                    member_addr, src_socket_addr, enroll_request.ipcp_name
                );
            }
        } else {
            println!("  ⚠ Member enrolled with address 0, skipping route creation");
        }

        Ok(())
    }

    /// Helper method to send enrollment response
    async fn send_enroll_response(
        &self,
        request_pdu: &Pdu,
        response: &EnrollmentResponse,
        request_cdap: &CdapMessage,
    ) -> Result<(), String> {
        // Serialize enrollment response
        let response_bytes = bincode::serialize(response)
            .map_err(|e| format!("Failed to serialize enrollment response: {}", e))?;

        // Create CDAP response message
        let cdap_response = CdapMessage {
            op_code: CdapOpCode::Create,
            obj_name: request_cdap.obj_name.clone(),
            obj_class: Some("enrollment".to_string()),
            obj_value: Some(RibValue::Bytes(response_bytes)),
            invoke_id: request_cdap.invoke_id,
            result: if response.accepted { 0 } else { 1 },
            result_reason: response.error.clone(),
        };

        // Serialize CDAP response
        let cdap_bytes = bincode::serialize(&cdap_response)
            .map_err(|e| format!("Failed to serialize CDAP response: {}", e))?;

        // Create response PDU
        let response_pdu = Pdu::new_data(
            self.local_addr,      // src_addr - bootstrap's address
            request_pdu.src_addr, // dst_addr - respond to sender
            0,                    // src_cep_id
            0,                    // dst_cep_id
            0,                    // sequence_num
            cdap_bytes,           // payload
        );

        // Send response
        self.shim
            .send_pdu(&response_pdu)
            .map_err(|e| format!("Failed to send enrollment response: {}", e))?;

        Ok(())
    }

    /// Handle incoming CDAP message (routes to appropriate handler)
    pub async fn handle_cdap_message(
        &self,
        pdu: &Pdu,
        src_socket_addr: SocketAddr,
    ) -> Result<(), String> {
        // Deserialize CDAP message from PDU payload
        let cdap_msg: CdapMessage = bincode::deserialize(&pdu.payload)
            .map_err(|e| format!("Failed to deserialize CDAP message: {}", e))?;

        // Route based on operation type and object class
        match (&cdap_msg.op_code, cdap_msg.obj_class.as_deref()) {
            // Enrollment request
            (CdapOpCode::Create, Some("enrollment")) => {
                self.handle_enrollment_request(pdu, src_socket_addr).await
            }
            // Routing table read request
            (CdapOpCode::Read, _) if cdap_msg.obj_name.starts_with("/routing/") => {
                self.handle_routing_read_request(pdu, &cdap_msg).await
            }
            // Unknown/unhandled message type
            _ => {
                // Silently ignore other message types for now
                Ok(())
            }
        }
    }

    /// Handle routing table read request
    async fn handle_routing_read_request(
        &self,
        pdu: &Pdu,
        request: &CdapMessage,
    ) -> Result<(), String> {
        // For now, return an empty routing table since member has static routes
        // In future phases, this could return actual routing information
        let response = CdapMessage {
            op_code: CdapOpCode::Read,
            obj_name: request.obj_name.clone(),
            obj_class: request.obj_class.clone(),
            obj_value: Some(RibValue::Struct(std::collections::HashMap::new())),
            invoke_id: request.invoke_id,
            result: 0,
            result_reason: None,
        };

        let response_bytes = bincode::serialize(&response)
            .map_err(|e| format!("Failed to serialize routing response: {}", e))?;

        let response_pdu = Pdu::new_data(self.local_addr, pdu.src_addr, 0, 0, 0, response_bytes);

        self.shim
            .send_pdu(&response_pdu)
            .map_err(|e| format!("Failed to send routing response: {}", e))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_enrollment_state() {
        let rib = Rib::new();
        let shim = Arc::new(UdpShim::new(0));
        let mut em = EnrollmentManager::new(rib, shim, 1000);

        assert_eq!(*em.state(), EnrollmentState::NotEnrolled);
        assert!(!em.is_enrolled());

        em.set_ipcp_name("ipcp-1".to_string());
        assert_eq!(*em.state(), EnrollmentState::Initiated);
    }
}
