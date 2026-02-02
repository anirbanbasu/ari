// SPDX-License-Identifier: EUPL-1.2-or-later
// Copyright © 2026-present ARI Contributors

//! IPCP Enrolment
//!
//! Handles the enrolment process where a new IPCP joins a DIF.
//! Includes state synchronization and RIB replication.

use crate::cdap::{CdapMessage, CdapOpCode, CdapSession};
use crate::efcp::{Efcp, FlowConfig};
use crate::rib::Rib;
use crate::shim::UdpShim;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::time::{SystemTime, UNIX_EPOCH};

/// Enrolment state
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnrolmentState {
    /// Not enrolled
    NotEnrolled,
    /// Enrolment initiated
    Initiated,
    /// Authenticating
    Authenticating,
    /// Synchronizing RIB
    Synchronizing,
    /// Enrolment complete
    Enrolled,
    /// Enrolment failed
    Failed(String),
}

/// Enrolment request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrolmentRequest {
    /// IPCP name requesting enrolment
    pub ipcp_name: String,
    /// IPCP address
    pub ipcp_address: u64,
    /// DIF name to join
    pub dif_name: String,
    /// Timestamp of request
    pub timestamp: u64,
}

/// Enrolment response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrolmentResponse {
    /// Whether enrolment was accepted
    pub accepted: bool,
    /// Error message if rejected
    pub error: Option<String>,
    /// DIF configuration if accepted
    pub dif_config: Option<DifConfiguration>,
}

/// DIF configuration provided during enrolment
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

/// Enrolment manager
#[derive(Debug)]
pub struct EnrolmentManager {
    /// Current enrolment state
    state: EnrolmentState,
    /// Local IPCP name
    ipcp_name: Option<String>,
    /// Local RIB
    rib: Rib,
}

impl EnrolmentManager {
    /// Creates a new enrolment manager
    pub fn new(rib: Rib) -> Self {
        Self {
            state: EnrolmentState::NotEnrolled,
            ipcp_name: None,
            rib,
        }
    }

    /// Initiates enrolment with a DIF
    pub fn initiate_enrolment(
        &mut self,
        ipcp_name: String,
        dif_name: String,
        ipcp_address: u64,
    ) -> EnrolmentRequest {
        self.state = EnrolmentState::Initiated;
        self.ipcp_name = Some(ipcp_name.clone());

        EnrolmentRequest {
            ipcp_name,
            ipcp_address,
            dif_name,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }

    /// Processes an enrolment request (called by accepting IPCP)
    pub fn process_enrolment_request(
        &self,
        request: EnrolmentRequest,
        dif_name: &str,
        neighbors: Vec<NeighborInfo>,
    ) -> EnrolmentResponse {
        // Validate DIF name
        if request.dif_name != dif_name {
            return EnrolmentResponse {
                accepted: false,
                error: Some(format!(
                    "DIF name mismatch: expected {}, got {}",
                    dif_name, request.dif_name
                )),
                dif_config: None,
            };
        }

        // Serialize the local RIB for the new member
        let rib_snapshot = self.rib.serialize();

        // Create DIF configuration
        let config = DifConfiguration {
            dif_name: dif_name.to_string(),
            assigned_address: request.ipcp_address,
            neighbors,
            rib_snapshot,
        };

        EnrolmentResponse {
            accepted: true,
            error: None,
            dif_config: Some(config),
        }
    }

    /// Completes enrolment after receiving response
    pub fn complete_enrolment(&mut self, response: EnrolmentResponse) -> Result<(), String> {
        if !response.accepted {
            self.state = EnrolmentState::Failed(
                response
                    .error
                    .unwrap_or_else(|| "Unknown error".to_string()),
            );
            return Err("Enrolment rejected".to_string());
        }

        // Synchronize RIB
        self.state = EnrolmentState::Synchronizing;

        if let Some(config) = response.dif_config {
            // Apply the RIB snapshot from the DIF
            if !config.rib_snapshot.is_empty() {
                match self.rib.deserialize(&config.rib_snapshot) {
                    Ok(count) => {
                        // Successfully synchronized RIB
                        if count > 0 {
                            // Objects were merged
                        }
                    }
                    Err(e) => {
                        self.state = EnrolmentState::Failed(format!("RIB sync failed: {}", e));
                        return Err(format!("Failed to synchronize RIB: {}", e));
                    }
                }
            }

            // Add neighbors to the RIB
            for neighbor in config.neighbors {
                let neighbor_name = format!("neighbor/{}", neighbor.name);
                let neighbor_data = crate::rib::RibValue::Struct(
                    vec![
                        (
                            "address".to_string(),
                            Box::new(crate::rib::RibValue::Integer(neighbor.address as i64)),
                        ),
                        (
                            "reachable".to_string(),
                            Box::new(crate::rib::RibValue::Boolean(neighbor.reachable)),
                        ),
                    ]
                    .into_iter()
                    .collect(),
                );

                // Create neighbor entry in RIB (ignore if already exists)
                let _ = self
                    .rib
                    .create(neighbor_name, "neighbor".to_string(), neighbor_data);
            }
        }

        self.state = EnrolmentState::Enrolled;
        Ok(())
    }

    /// Returns the current enrolment state
    pub fn state(&self) -> &EnrolmentState {
        &self.state
    }

    /// Checks if enrolled
    pub fn is_enrolled(&self) -> bool {
        self.state == EnrolmentState::Enrolled
    }

    /// Resets enrolment state
    pub fn reset(&mut self) {
        self.state = EnrolmentState::NotEnrolled;
        self.ipcp_name = None;
    }

    // ========== Network Enrolment Methods (Phase 1) ==========

    /// Allocates a management flow to bootstrap IPCP for enrolment
    pub fn allocate_management_flow(
        &mut self,
        _bootstrap_socket_addr: SocketAddr,
        local_addr: u64,
        bootstrap_rina_addr: u64,
        efcp: &mut Efcp,
        _shim: &UdpShim,
    ) -> Result<u32, String> {
        // Create management flow configuration with reliable, ordered delivery
        let config = FlowConfig {
            max_pdu_size: 1500,
            window_size: 32,
            reliable: true,
            retransmit_timeout_ms: 2000,
        };

        // Allocate flow via EFCP
        // Use temporary address 0 if not yet assigned
        let src_addr = if local_addr == 0 { 0 } else { local_addr };
        let flow_id = efcp.allocate_flow(src_addr, bootstrap_rina_addr, config);

        // Store bootstrap address for sending via shim
        // In a real implementation, we'd register this mapping in the shim
        // For now, the socket address mapping will be handled by the caller

        Ok(flow_id)
    }

    /// Sends enrolment request via CDAP over EFCP
    pub fn send_enrolment_request(
        &mut self,
        flow_id: u32,
        request: &EnrolmentRequest,
        cdap: &mut CdapSession,
        efcp: &mut Efcp,
    ) -> Result<u64, String> {
        // Serialize enrolment request
        let request_json = serde_json::to_string(request)
            .map_err(|e| format!("Failed to serialize enrolment request: {}", e))?;

        // Create CDAP message for enrolment request
        let cdap_msg = cdap.create_request(
            "enrolment/request".to_string(),
            "EnrolmentRequest".to_string(),
            crate::rib::RibValue::String(request_json),
        );

        let invoke_id = cdap_msg.invoke_id;

        // Serialize CDAP message
        let cdap_json = serialize_cdap_message(&cdap_msg)?;

        // Send via EFCP
        let flow = efcp
            .get_flow_mut(flow_id)
            .ok_or_else(|| format!("Flow {} not found", flow_id))?;

        let _pdu = flow.send_data(cdap_json.into_bytes())?;

        self.state = EnrolmentState::Authenticating;
        Ok(invoke_id)
    }

    /// Receives and processes enrolment response via CDAP over EFCP
    pub fn receive_enrolment_response(
        &mut self,
        flow_id: u32,
        _expected_invoke_id: u64,
        efcp: &mut Efcp,
    ) -> Result<EnrolmentResponse, String> {
        // Note: In a real implementation, this would be async and wait for data
        // For now, this is a synchronous placeholder that should be called
        // when data is available on the flow

        // Get flow and check if data is available in receive buffer
        let _flow = efcp
            .get_flow_mut(flow_id)
            .ok_or_else(|| format!("Flow {} not found", flow_id))?;

        // In a real implementation, we'd wait for PDU reception
        // For now, return an error indicating data not yet received
        // This will be properly implemented with async/await in Phase 2

        Err("Response not yet available (async implementation pending)".to_string())
    }

    /// Processes incoming enrolment request from member IPCP (bootstrap side)
    pub fn handle_enrolment_request(
        &self,
        flow_id: u32,
        cdap_msg: &CdapMessage,
        dif_name: &str,
        neighbors: Vec<NeighborInfo>,
        _cdap: &mut CdapSession,
        efcp: &mut Efcp,
    ) -> Result<(), String> {
        // Verify this is an enrolment request
        if cdap_msg.obj_name != "enrolment/request" {
            return Err(format!(
                "Expected enrolment/request, got {}",
                cdap_msg.obj_name
            ));
        }

        // Extract enrolment request from CDAP message
        let request_json = cdap_msg
            .obj_value
            .as_ref()
            .and_then(|v| v.as_string())
            .ok_or("Missing enrolment request data")?;

        let request: EnrolmentRequest = serde_json::from_str(request_json)
            .map_err(|e| format!("Failed to parse enrolment request: {}", e))?;

        // Process the request
        let response = self.process_enrolment_request(request, dif_name, neighbors);

        // Serialize response
        let response_json = serde_json::to_string(&response)
            .map_err(|e| format!("Failed to serialize enrolment response: {}", e))?;

        // Create CDAP response
        let mut cdap_response = CdapMessage::new_response(cdap_msg.invoke_id, 0, None);
        cdap_response.obj_value = Some(crate::rib::RibValue::String(response_json));
        cdap_response.obj_name = "enrolment/response".to_string();
        cdap_response.op_code = CdapOpCode::Create;

        // Serialize and send response
        let response_data = serialize_cdap_message(&cdap_response)?;

        let flow = efcp
            .get_flow_mut(flow_id)
            .ok_or_else(|| format!("Flow {} not found", flow_id))?;

        flow.send_data(response_data.into_bytes())?;

        Ok(())
    }
}

// ========== CDAP Serialization Helpers ==========

/// Serializes a CDAP message to JSON format
fn serialize_cdap_message(msg: &CdapMessage) -> Result<String, String> {
    // Create a simplified representation for serialization
    let simplified = serde_json::json!({
        "op_code": format!("{:?}", msg.op_code),
        "obj_name": msg.obj_name,
        "obj_class": msg.obj_class,
        "obj_value": msg.obj_value.as_ref().map(|v| format!("{:?}", v)),
        "invoke_id": msg.invoke_id,
        "result": msg.result,
        "result_reason": msg.result_reason,
    });

    serde_json::to_string(&simplified)
        .map_err(|e| format!("Failed to serialize CDAP message: {}", e))
}

/// Deserializes a CDAP message from JSON format
#[allow(dead_code)]
fn deserialize_cdap_message(_data: &str) -> Result<CdapMessage, String> {
    // This is a placeholder - proper implementation would parse JSON
    // and reconstruct the CDAP message
    Err("CDAP deserialization not yet fully implemented".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enrolment_initiate() {
        let rib = Rib::new();
        let mut em = EnrolmentManager::new(rib);

        let request = em.initiate_enrolment("ipcp-1".to_string(), "dif-1".to_string(), 1000);

        assert_eq!(request.ipcp_name, "ipcp-1");
        assert_eq!(request.dif_name, "dif-1");
        assert_eq!(*em.state(), EnrolmentState::Initiated);
    }

    #[test]
    fn test_enrolment_process_request() {
        let rib = Rib::new();
        let em = EnrolmentManager::new(rib);

        let request = EnrolmentRequest {
            ipcp_name: "ipcp-1".to_string(),
            ipcp_address: 1000,
            dif_name: "dif-1".to_string(),
            timestamp: 0,
        };

        let response = em.process_enrolment_request(request, "dif-1", vec![]);

        assert!(response.accepted);
        assert!(response.dif_config.is_some());
    }

    #[test]
    fn test_enrolment_dif_mismatch() {
        let rib = Rib::new();
        let em = EnrolmentManager::new(rib);

        let request = EnrolmentRequest {
            ipcp_name: "ipcp-1".to_string(),
            ipcp_address: 1000,
            dif_name: "dif-1".to_string(),
            timestamp: 0,
        };

        let response = em.process_enrolment_request(request, "dif-2", vec![]);

        assert!(!response.accepted);
        assert!(response.error.is_some());
    }

    #[test]
    fn test_enrolment_complete() {
        let rib = Rib::new();
        let mut em = EnrolmentManager::new(rib);

        let config = DifConfiguration {
            dif_name: "dif-1".to_string(),
            assigned_address: 1000,
            neighbors: vec![],
            rib_snapshot: vec![],
        };

        let response = EnrolmentResponse {
            accepted: true,
            error: None,
            dif_config: Some(config),
        };

        em.complete_enrolment(response).unwrap();
        assert!(em.is_enrolled());
    }
}
