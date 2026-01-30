// SPDX-License-Identifier: EUPL-1.2-or-later
// Copyright © 2026-present RINA (Rust) Contributors

//! IPCP Enrollment
//!
//! Handles the enrollment process where a new IPCP joins a DIF.
//! Includes state synchronization and RIB replication.

use crate::rib::Rib;
use std::time::{SystemTime, UNIX_EPOCH};

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
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone)]
pub struct EnrollmentResponse {
    /// Whether enrollment was accepted
    pub accepted: bool,
    /// Error message if rejected
    pub error: Option<String>,
    /// DIF configuration if accepted
    pub dif_config: Option<DifConfiguration>,
}

/// DIF configuration provided during enrollment
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone)]
pub struct NeighborInfo {
    /// Neighbor IPCP name
    pub name: String,
    /// Neighbor address
    pub address: u64,
    /// Whether this neighbor is currently reachable
    pub reachable: bool,
}

/// Enrollment manager
#[derive(Debug)]
pub struct EnrollmentManager {
    /// Current enrollment state
    state: EnrollmentState,
    /// Local IPCP name
    ipcp_name: Option<String>,
    /// Local RIB
    rib: Rib,
}

impl EnrollmentManager {
    /// Creates a new enrollment manager
    pub fn new(rib: Rib) -> Self {
        Self {
            state: EnrollmentState::NotEnrolled,
            ipcp_name: None,
            rib,
        }
    }

    /// Initiates enrollment with a DIF
    pub fn initiate_enrollment(
        &mut self,
        ipcp_name: String,
        dif_name: String,
        ipcp_address: u64,
    ) -> EnrollmentRequest {
        self.state = EnrollmentState::Initiated;
        self.ipcp_name = Some(ipcp_name.clone());

        EnrollmentRequest {
            ipcp_name,
            ipcp_address,
            dif_name,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }

    /// Processes an enrollment request (called by accepting IPCP)
    pub fn process_enrollment_request(
        &self,
        request: EnrollmentRequest,
        dif_name: &str,
        neighbors: Vec<NeighborInfo>,
    ) -> EnrollmentResponse {
        // Validate DIF name
        if request.dif_name != dif_name {
            return EnrollmentResponse {
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

        EnrollmentResponse {
            accepted: true,
            error: None,
            dif_config: Some(config),
        }
    }

    /// Completes enrollment after receiving response
    pub fn complete_enrollment(&mut self, response: EnrollmentResponse) -> Result<(), String> {
        if !response.accepted {
            self.state = EnrollmentState::Failed(
                response
                    .error
                    .unwrap_or_else(|| "Unknown error".to_string()),
            );
            return Err("Enrollment rejected".to_string());
        }

        // Synchronize RIB
        self.state = EnrollmentState::Synchronizing;

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
                        self.state = EnrollmentState::Failed(format!("RIB sync failed: {}", e));
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

        self.state = EnrollmentState::Enrolled;
        Ok(())
    }

    /// Returns the current enrollment state
    pub fn state(&self) -> &EnrollmentState {
        &self.state
    }

    /// Checks if enrolled
    pub fn is_enrolled(&self) -> bool {
        self.state == EnrollmentState::Enrolled
    }

    /// Resets enrollment state
    pub fn reset(&mut self) {
        self.state = EnrollmentState::NotEnrolled;
        self.ipcp_name = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enrollment_initiate() {
        let rib = Rib::new();
        let mut em = EnrollmentManager::new(rib);

        let request = em.initiate_enrollment("ipcp-1".to_string(), "dif-1".to_string(), 1000);

        assert_eq!(request.ipcp_name, "ipcp-1");
        assert_eq!(request.dif_name, "dif-1");
        assert_eq!(*em.state(), EnrollmentState::Initiated);
    }

    #[test]
    fn test_enrollment_process_request() {
        let rib = Rib::new();
        let em = EnrollmentManager::new(rib);

        let request = EnrollmentRequest {
            ipcp_name: "ipcp-1".to_string(),
            ipcp_address: 1000,
            dif_name: "dif-1".to_string(),
            timestamp: 0,
        };

        let response = em.process_enrollment_request(request, "dif-1", vec![]);

        assert!(response.accepted);
        assert!(response.dif_config.is_some());
    }

    #[test]
    fn test_enrollment_dif_mismatch() {
        let rib = Rib::new();
        let em = EnrollmentManager::new(rib);

        let request = EnrollmentRequest {
            ipcp_name: "ipcp-1".to_string(),
            ipcp_address: 1000,
            dif_name: "dif-1".to_string(),
            timestamp: 0,
        };

        let response = em.process_enrollment_request(request, "dif-2", vec![]);

        assert!(!response.accepted);
        assert!(response.error.is_some());
    }

    #[test]
    fn test_enrollment_complete() {
        let rib = Rib::new();
        let mut em = EnrollmentManager::new(rib);

        let config = DifConfiguration {
            dif_name: "dif-1".to_string(),
            assigned_address: 1000,
            neighbors: vec![],
            rib_snapshot: vec![],
        };

        let response = EnrollmentResponse {
            accepted: true,
            error: None,
            dif_config: Some(config),
        };

        em.complete_enrollment(response).unwrap();
        assert!(em.is_enrolled());
    }
}
