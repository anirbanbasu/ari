// SPDX-License-Identifier: EUPL-1.2-or-later
// Copyright © 2026-present ARI Contributors

//! Protocol Data Unit (PDU) definitions
//!
//! Common PDU structures used across RINA components.
//! Consolidated from various modules for consistency.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Protocol Data Unit (PDU) - the basic unit of data transfer
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pdu {
    /// Source address
    pub src_addr: u64,
    /// Destination address
    pub dst_addr: u64,
    /// Source Connection Endpoint ID (CEP-ID)
    pub src_cep_id: u32,
    /// Destination Connection Endpoint ID (CEP-ID)
    pub dst_cep_id: u32,
    /// Sequence number for ordering and flow control
    pub sequence_num: u64,
    /// PDU type (data, ack, control)
    pub pdu_type: PduType,
    /// Payload data
    pub payload: Vec<u8>,
    /// Quality of Service (QoS) parameters
    pub qos: QoSParameters,
}

/// Types of PDUs
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PduType {
    /// Data transfer PDU
    Data,
    /// Acknowledgment PDU
    Ack,
    /// Control PDU (e.g., flow control)
    Control,
    /// Management PDU (for enrollment, etc.)
    Management,
}

impl fmt::Display for PduType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PduType::Data => write!(f, "DATA"),
            PduType::Ack => write!(f, "ACK"),
            PduType::Control => write!(f, "CONTROL"),
            PduType::Management => write!(f, "MANAGEMENT"),
        }
    }
}

/// Quality of Service parameters for PDUs
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QoSParameters {
    /// Priority level (0-255, higher is more important)
    pub priority: u8,
    /// Maximum delay tolerance (milliseconds)
    pub max_delay_ms: Option<u32>,
    /// Minimum bandwidth requirement (bytes/sec)
    pub min_bandwidth_bps: Option<u64>,
    /// Maximum loss rate (0-100)
    pub max_loss_rate: Option<u8>,
}

impl Default for QoSParameters {
    fn default() -> Self {
        Self {
            priority: 128, // Medium priority
            max_delay_ms: None,
            min_bandwidth_bps: None,
            max_loss_rate: None,
        }
    }
}

impl Pdu {
    /// Creates a new data PDU
    pub fn new_data(
        src_addr: u64,
        dst_addr: u64,
        src_cep_id: u32,
        dst_cep_id: u32,
        sequence_num: u64,
        payload: Vec<u8>,
    ) -> Self {
        Self {
            src_addr,
            dst_addr,
            src_cep_id,
            dst_cep_id,
            sequence_num,
            pdu_type: PduType::Data,
            payload,
            qos: QoSParameters::default(),
        }
    }

    /// Creates a new data PDU with QoS parameters
    pub fn new_data_with_qos(
        src_addr: u64,
        dst_addr: u64,
        src_cep_id: u32,
        dst_cep_id: u32,
        sequence_num: u64,
        payload: Vec<u8>,
        qos: QoSParameters,
    ) -> Self {
        Self {
            src_addr,
            dst_addr,
            src_cep_id,
            dst_cep_id,
            sequence_num,
            pdu_type: PduType::Data,
            payload,
            qos,
        }
    }

    /// Creates a new ACK PDU
    pub fn new_ack(
        src_addr: u64,
        dst_addr: u64,
        src_cep_id: u32,
        dst_cep_id: u32,
        ack_num: u64,
    ) -> Self {
        Self {
            src_addr,
            dst_addr,
            src_cep_id,
            dst_cep_id,
            sequence_num: ack_num,
            pdu_type: PduType::Ack,
            payload: Vec::new(),
            qos: QoSParameters::default(),
        }
    }

    /// Creates a new management PDU
    pub fn new_management(src_addr: u64, dst_addr: u64, payload: Vec<u8>) -> Self {
        Self {
            src_addr,
            dst_addr,
            src_cep_id: 0,
            dst_cep_id: 0,
            sequence_num: 0,
            pdu_type: PduType::Management,
            payload,
            qos: QoSParameters::default(),
        }
    }

    /// Returns the total size of the PDU in bytes
    pub fn size(&self) -> usize {
        // Header size + payload size
        // Simplified: 8 + 8 + 4 + 4 + 8 + 1 (type) + payload
        33 + self.payload.len()
    }

    /// Checks if this is a data PDU
    pub fn is_data(&self) -> bool {
        self.pdu_type == PduType::Data
    }

    /// Checks if this is an ACK PDU
    pub fn is_ack(&self) -> bool {
        self.pdu_type == PduType::Ack
    }

    /// Checks if this is a management PDU
    pub fn is_management(&self) -> bool {
        self.pdu_type == PduType::Management
    }

    /// Serializes the PDU to bytes using bincode
    pub fn serialize(&self) -> Result<Vec<u8>, String> {
        bincode::serialize(self).map_err(|e| format!("Failed to serialize PDU: {}", e))
    }

    /// Deserializes a PDU from bytes using bincode
    pub fn deserialize(data: &[u8]) -> Result<Self, String> {
        bincode::deserialize(data).map_err(|e| format!("Failed to deserialize PDU: {}", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pdu_creation() {
        let pdu = Pdu::new_data(100, 200, 1, 2, 0, vec![1, 2, 3, 4]);
        assert_eq!(pdu.src_addr, 100);
        assert_eq!(pdu.dst_addr, 200);
        assert_eq!(pdu.sequence_num, 0);
        assert!(pdu.is_data());
    }

    #[test]
    fn test_pdu_with_qos() {
        let qos = QoSParameters {
            priority: 200,
            max_delay_ms: Some(100),
            min_bandwidth_bps: Some(1000000),
            max_loss_rate: Some(5),
        };

        let pdu = Pdu::new_data_with_qos(100, 200, 1, 2, 0, vec![1, 2, 3], qos.clone());
        assert_eq!(pdu.qos.priority, 200);
        assert_eq!(pdu.qos.max_delay_ms, Some(100));
    }

    #[test]
    fn test_pdu_types() {
        let data_pdu = Pdu::new_data(1, 2, 1, 2, 0, vec![]);
        let ack_pdu = Pdu::new_ack(1, 2, 1, 2, 5);
        let mgmt_pdu = Pdu::new_management(1, 2, vec![]);

        assert!(data_pdu.is_data());
        assert!(ack_pdu.is_ack());
        assert!(mgmt_pdu.is_management());
    }

    #[test]
    fn test_pdu_size() {
        let pdu = Pdu::new_data(1, 2, 1, 2, 0, vec![0; 100]);
        assert_eq!(pdu.size(), 133); // 33 byte header + 100 byte payload
    }
}
