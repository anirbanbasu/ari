// SPDX-License-Identifier: EUPL-1.2-or-later
// Copyright © 2026-present ARI Contributors

//! IPCP Enrollment
//!
//! Handles the enrollment process where a new IPCP joins a DIF.
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
    /// IPCP address
    pub ipcp_address: u64,
    /// DIF name to join
    pub dif_name: String,
    /// Timestamp of request
    pub timestamp: u64,
}

/// Enrollment response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrollmentResponse {
    /// Whether enrollment was accepted
    pub accepted: bool,
    /// Error message if rejected
    pub error: Option<String>,
    /// DIF configuration if accepted
    pub dif_config: Option<DifConfiguration>,
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
    /// Local RIB
    rib: Rib,
    /// UDP shim for network communication
    shim: Arc<UdpShim>,
    /// Enrollment configuration
    config: EnrollmentConfig,
}

impl EnrollmentManager {
    /// Creates a new enrollment manager
    pub fn new(rib: Rib, shim: Arc<UdpShim>) -> Self {
        Self::with_config(rib, shim, EnrollmentConfig::default())
    }

    /// Creates a new enrollment manager with custom configuration
    pub fn with_config(rib: Rib, shim: Arc<UdpShim>, config: EnrollmentConfig) -> Self {
        Self {
            state: EnrollmentState::NotEnrolled,
            ipcp_name: None,
            rib,
            shim,
            config,
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

        // Create enrollment request CDAP message
        let cdap_msg = CdapMessage {
            op_code: CdapOpCode::Create,
            obj_name: ipcp_name.clone(),
            obj_class: Some("enrollment".to_string()),
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

        // Send enrollment request
        self.shim
            .send_pdu(&pdu)
            .map_err(|e| format!("Failed to send enrollment request: {}", e))?;

        println!("Sent enrollment request to bootstrap IPCP");

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

        Ok(dif_name)
    }

    /// Receive enrollment response with polling
    async fn receive_response(&self) -> Result<CdapMessage, String> {
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

                // Check if this is an enrollment response
                if cdap_msg.obj_class.as_deref() == Some("enrollment") {
                    if cdap_msg.result == 0 {
                        return Ok(cdap_msg);
                    } else {
                        return Err(format!(
                            "Enrollment rejected with code: {}",
                            cdap_msg.result
                        ));
                    }
                }
            }

            sleep(poll_interval).await;
        }

        Err("No enrollment response received".to_string())
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

        let requesting_ipcp = cdap_msg
            .obj_value
            .as_ref()
            .and_then(|v| v.as_string())
            .ok_or("Request does not contain IPCP name")?
            .to_string();

        println!("Received enrollment request from: {}", requesting_ipcp);

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

        // Create response CDAP message
        let response = CdapMessage {
            op_code: CdapOpCode::Create,
            obj_name: requesting_ipcp.clone(),
            obj_class: Some("enrollment".to_string()),
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
            .map_err(|e| format!("Failed to send enrollment response: {}", e))?;

        println!(
            "Sent enrollment response to {} with DIF name: {}",
            requesting_ipcp, dif_name
        );

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
        let mut em = EnrollmentManager::new(rib, shim);

        assert_eq!(*em.state(), EnrollmentState::NotEnrolled);
        assert!(!em.is_enrolled());

        em.set_ipcp_name("ipcp-1".to_string());
        assert_eq!(*em.state(), EnrollmentState::Initiated);
    }
}
