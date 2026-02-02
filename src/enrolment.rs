// SPDX-License-Identifier: EUPL-1.2-or-later
// Copyright © 2026-present ARI Contributors

//! IPCP Enrolment
//!
//! Handles the enrolment process where a new IPCP joins a DIF.
//! Fully async implementation with timeout and retry logic.

use crate::cdap::{CdapMessage, CdapOpCode};
use crate::pdu::Pdu;
use crate::rib::{Rib, RibValue};
use crate::shim::UdpShim;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::{sleep, timeout};

const ENROLMENT_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_RETRY_ATTEMPTS: u32 = 3;
const RETRY_BACKOFF_MS: u64 = 1000;

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

/// Enrolment manager - fully async implementation
#[derive(Debug)]
pub struct EnrolmentManager {
    /// Current enrolment state
    state: EnrolmentState,
    /// Local IPCP name
    ipcp_name: Option<String>,
    /// Local RIB
    rib: Rib,
    /// UDP shim for network communication
    shim: Arc<UdpShim>,
}

impl EnrolmentManager {
    /// Creates a new enrolment manager
    pub fn new(rib: Rib, shim: Arc<UdpShim>) -> Self {
        Self {
            state: EnrolmentState::NotEnrolled,
            ipcp_name: None,
            rib,
            shim,
        }
    }

    /// Sets the IPCP name
    pub fn set_ipcp_name(&mut self, name: String) {
        self.ipcp_name = Some(name);
        self.state = EnrolmentState::Initiated;
    }

    /// Returns the current enrolment state
    pub fn state(&self) -> &EnrolmentState {
        &self.state
    }

    /// Checks if enrolled
    pub fn is_enrolled(&self) -> bool {
        self.state == EnrolmentState::Enrolled
    }

    /// Enrol with bootstrap IPCP with timeout and retry logic
    pub async fn enrol_with_bootstrap(&mut self, bootstrap_addr: u64) -> Result<String, String> {
        for attempt in 1..=MAX_RETRY_ATTEMPTS {
            println!("Enrolment attempt {}/{}", attempt, MAX_RETRY_ATTEMPTS);

            match timeout(ENROLMENT_TIMEOUT, self.try_enrol(bootstrap_addr)).await {
                Ok(Ok(dif_name)) => {
                    println!("Successfully enrolled in DIF: {}", dif_name);
                    return Ok(dif_name);
                }
                Ok(Err(e)) => {
                    eprintln!("Enrolment attempt {} failed: {}", attempt, e);
                }
                Err(_) => {
                    eprintln!("Enrolment attempt {} timed out", attempt);
                }
            }

            if attempt < MAX_RETRY_ATTEMPTS {
                let backoff = Duration::from_millis(RETRY_BACKOFF_MS * (1 << (attempt - 1)));
                println!("Retrying in {:?}...", backoff);
                sleep(backoff).await;
            }
        }

        Err(format!(
            "Enrolment failed after {} attempts",
            MAX_RETRY_ATTEMPTS
        ))
    }

    /// Single enrolment attempt
    async fn try_enrol(&mut self, bootstrap_addr: u64) -> Result<String, String> {
        let ipcp_name = self.ipcp_name.as_ref().ok_or("IPCP name not set")?.clone();

        // Create enrolment request CDAP message
        let cdap_msg = CdapMessage {
            op_code: CdapOpCode::Create,
            obj_name: ipcp_name.clone(),
            obj_class: Some("enrolment".to_string()),
            obj_value: Some(RibValue::String(ipcp_name.clone())),
            invoke_id: 1,
            result: 0,
            result_reason: None,
        };

        // Serialize CDAP message with bincode
        let cdap_bytes = bincode::serialize(&cdap_msg)
            .map_err(|e| format!("Failed to serialize CDAP message: {}", e))?;

        // Create PDU with CDAP payload
        let pdu = Pdu::new_data(
            0,              // src_addr - member doesn't have address yet
            bootstrap_addr, // dst_addr
            0,              // src_cep_id
            0,              // dst_cep_id
            0,              // sequence_num
            cdap_bytes,     // payload
        );

        // Send enrolment request
        self.shim
            .send_pdu(&pdu)
            .map_err(|e| format!("Failed to send enrolment request: {}", e))?;

        println!("Sent enrolment request to bootstrap IPCP");

        // Wait for response
        let response = self.receive_response().await?;

        // Extract DIF name from response
        let dif_name = response
            .obj_value
            .as_ref()
            .and_then(|v| v.as_string())
            .ok_or("Response does not contain DIF name")?
            .to_string();

        // Update state
        self.state = EnrolmentState::Enrolled;

        // Store DIF name in RIB
        let _ = self.rib.create(
            "/dif/name".to_string(),
            "dif_info".to_string(),
            RibValue::String(dif_name.clone()),
        );

        Ok(dif_name)
    }

    /// Receive enrolment response with polling
    async fn receive_response(&self) -> Result<CdapMessage, String> {
        let poll_interval = Duration::from_millis(100);
        let max_polls = (ENROLMENT_TIMEOUT.as_millis() / poll_interval.as_millis()) as u32;

        for _ in 0..max_polls {
            if let Some((pdu, _src_addr)) = self
                .shim
                .receive_pdu()
                .map_err(|e| format!("Failed to receive PDU: {}", e))?
            {
                // Deserialize CDAP message from PDU payload
                let cdap_msg: CdapMessage = bincode::deserialize(&pdu.payload)
                    .map_err(|e| format!("Failed to deserialize CDAP message: {}", e))?;

                // Check if this is an enrolment response
                if cdap_msg.obj_class.as_deref() == Some("enrolment") {
                    if cdap_msg.result == 0 {
                        return Ok(cdap_msg);
                    } else {
                        return Err(format!("Enrolment rejected with code: {}", cdap_msg.result));
                    }
                }
            }

            sleep(poll_interval).await;
        }

        Err("No enrolment response received".to_string())
    }

    /// Handle incoming enrolment request (bootstrap side)
    pub async fn handle_enrolment_request(
        &self,
        pdu: &Pdu,
        src_socket_addr: SocketAddr,
    ) -> Result<(), String> {
        // Register the peer mapping so we can send response back
        self.shim.register_peer(pdu.src_addr, src_socket_addr);

        // Deserialize CDAP message from PDU payload
        let cdap_msg: CdapMessage = bincode::deserialize(&pdu.payload)
            .map_err(|e| format!("Failed to deserialize CDAP message: {}", e))?;

        // Check if this is an enrolment request
        if cdap_msg.obj_class.as_deref() != Some("enrolment")
            || cdap_msg.op_code != CdapOpCode::Create
        {
            return Err("Not an enrolment request".to_string());
        }

        let requesting_ipcp = cdap_msg
            .obj_value
            .as_ref()
            .and_then(|v| v.as_string())
            .ok_or("Request does not contain IPCP name")?
            .to_string();

        println!("Received enrolment request from: {}", requesting_ipcp);

        // Get DIF name from RIB
        let dif_name_obj = self
            .rib
            .read("/dif/name")
            .ok_or("Bootstrap DIF name not set in RIB")?;
        let dif_name = dif_name_obj
            .value
            .as_string()
            .ok_or("DIF name is not a string")?
            .to_string();

        // Create response CDAP message
        let response = CdapMessage {
            op_code: CdapOpCode::Create,
            obj_name: requesting_ipcp.clone(),
            obj_class: Some("enrolment".to_string()),
            obj_value: Some(RibValue::String(dif_name.clone())),
            invoke_id: cdap_msg.invoke_id,
            result: 0, // Success
            result_reason: None,
        };

        // Serialize response
        let response_bytes = bincode::serialize(&response)
            .map_err(|e| format!("Failed to serialize response: {}", e))?;

        // Create response PDU
        let response_pdu = Pdu::new_data(
            0,              // src_addr
            pdu.src_addr,   // dst_addr - respond to sender
            0,              // src_cep_id
            0,              // dst_cep_id
            0,              // sequence_num
            response_bytes, // payload
        );

        // Send response
        self.shim
            .send_pdu(&response_pdu)
            .map_err(|e| format!("Failed to send enrolment response: {}", e))?;

        println!(
            "Sent enrolment response to {} with DIF name: {}",
            requesting_ipcp, dif_name
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_enrolment_state() {
        let rib = Rib::new();
        let shim = Arc::new(UdpShim::new(0));
        let mut em = EnrolmentManager::new(rib, shim);

        assert_eq!(*em.state(), EnrolmentState::NotEnrolled);
        assert!(!em.is_enrolled());

        em.set_ipcp_name("ipcp-1".to_string());
        assert_eq!(*em.state(), EnrolmentState::Initiated);
    }
}
