// SPDX-License-Identifier: EUPL-1.2-or-later
// Copyright © 2026-present ARI Contributors

//! Error and Flow Control Protocol (EFCP)
//!
//! EFCP provides reliable and unreliable data transfer with flow control,
//! error detection, and retransmission capabilities. It's the core data
//! transfer protocol in RINA.

use crate::pdu::{Pdu, PduType};
use std::collections::{HashMap, VecDeque};
use std::time::{SystemTime, UNIX_EPOCH};

/// Flow state and configuration
#[derive(Debug, Clone)]
pub struct FlowConfig {
    /// Maximum PDU size
    pub max_pdu_size: usize,
    /// Window size for flow control
    pub window_size: u64,
    /// Whether to use reliable transfer (ACKs and retransmission)
    pub reliable: bool,
    /// Timeout for retransmission (milliseconds)
    pub retransmit_timeout_ms: u64,
}

impl Default for FlowConfig {
    fn default() -> Self {
        Self {
            max_pdu_size: 1500,
            window_size: 64,
            reliable: true,
            retransmit_timeout_ms: 1000,
        }
    }
}

/// Represents a flow connection
#[derive(Debug)]
pub struct Flow {
    /// Flow identifier (port-id)
    pub flow_id: u32,
    /// Local CEP-ID
    pub local_cep_id: u32,
    /// Remote CEP-ID
    pub remote_cep_id: u32,
    /// Local address
    pub local_addr: u64,
    /// Remote address
    pub remote_addr: u64,
    /// Flow configuration
    pub config: FlowConfig,
    /// Next sequence number to send
    next_seq_num: u64,
    /// Expected next sequence number to receive
    expected_seq_num: u64,
    /// Send window: PDUs sent but not yet ACKed
    send_window: HashMap<u64, (Pdu, u64)>, // (PDU, timestamp)
    /// Receive buffer for out-of-order PDUs
    receive_buffer: VecDeque<Pdu>,
}

impl Flow {
    /// Creates a new flow
    pub fn new(
        flow_id: u32,
        local_cep_id: u32,
        remote_cep_id: u32,
        local_addr: u64,
        remote_addr: u64,
        config: FlowConfig,
    ) -> Self {
        Self {
            flow_id,
            local_cep_id,
            remote_cep_id,
            local_addr,
            remote_addr,
            config,
            next_seq_num: 0,
            expected_seq_num: 0,
            send_window: HashMap::new(),
            receive_buffer: VecDeque::new(),
        }
    }

    /// Prepares a PDU for sending data
    pub fn send_data(&mut self, payload: Vec<u8>) -> Result<Pdu, String> {
        if payload.len() > self.config.max_pdu_size {
            return Err(format!(
                "Payload size {} exceeds max PDU size {}",
                payload.len(),
                self.config.max_pdu_size
            ));
        }

        if self.send_window.len() >= self.config.window_size as usize {
            return Err("Send window is full".to_string());
        }

        let pdu = Pdu::new_data(
            self.local_addr,
            self.remote_addr,
            self.local_cep_id,
            self.remote_cep_id,
            self.next_seq_num,
            payload,
        );

        if self.config.reliable {
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;
            self.send_window
                .insert(self.next_seq_num, (pdu.clone(), timestamp));
        }

        self.next_seq_num += 1;
        Ok(pdu)
    }

    fn handle_data_pdu(&mut self, pdu: Pdu) -> Result<Option<Vec<u8>>, String> {
        if pdu.sequence_num == self.expected_seq_num {
            // In-order PDU
            self.expected_seq_num += 1;

            if self.config.reliable {
                // Generate ACK (caller should send this)
                // In a real implementation, we'd queue this for sending
            }

            Ok(Some(pdu.payload))
        } else if pdu.sequence_num > self.expected_seq_num {
            // Out-of-order PDU - buffer it
            self.receive_buffer.push_back(pdu);
            Ok(None)
        } else {
            // Duplicate or old PDU - discard
            Ok(None)
        }
    }

    fn handle_ack_pdu(&mut self, pdu: Pdu) -> Result<Option<Vec<u8>>, String> {
        let ack_num = pdu.sequence_num;

        // Remove ACKed PDUs from send window
        self.send_window.retain(|seq_num, _| *seq_num > ack_num);

        Ok(None)
    }

    fn handle_control_pdu(&mut self, _pdu: Pdu) -> Result<Option<Vec<u8>>, String> {
        // TODO: Handle control PDUs (e.g., flow control updates)
        Ok(None)
    }

    fn handle_management_pdu(&mut self, _pdu: Pdu) -> Result<Option<Vec<u8>>, String> {
        // Management PDUs should be handled by enrolment/cdap layers
        Ok(None)
    }

    /// Processes a received PDU
    pub fn receive_pdu(&mut self, pdu: Pdu) -> Result<Option<Vec<u8>>, String> {
        match pdu.pdu_type {
            PduType::Data => self.handle_data_pdu(pdu),
            PduType::Ack => self.handle_ack_pdu(pdu),
            PduType::Control => self.handle_control_pdu(pdu),
            PduType::Management => self.handle_management_pdu(pdu),
        }
    }

    /// Checks for PDUs that need retransmission
    pub fn check_retransmits(&self) -> Vec<Pdu> {
        if !self.config.reliable {
            return Vec::new();
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        self.send_window
            .values()
            .filter(|(_, timestamp)| now - timestamp > self.config.retransmit_timeout_ms)
            .map(|(pdu, _)| pdu.clone())
            .collect()
    }

    /// Returns the current send window size
    pub fn send_window_size(&self) -> usize {
        self.send_window.len()
    }
}

/// EFCP instance managing multiple flows
#[derive(Debug)]
pub struct Efcp {
    /// Active flows, keyed by flow ID
    flows: HashMap<u32, Flow>,
    /// Next available flow ID
    next_flow_id: u32,
}

impl Efcp {
    /// Creates a new EFCP instance
    pub fn new() -> Self {
        Self {
            flows: HashMap::new(),
            next_flow_id: 1,
        }
    }

    /// Allocates a new flow
    pub fn allocate_flow(&mut self, local_addr: u64, remote_addr: u64, config: FlowConfig) -> u32 {
        let flow_id = self.next_flow_id;
        self.next_flow_id += 1;

        let flow = Flow::new(
            flow_id,
            flow_id, // Using flow_id as CEP-ID for simplicity
            0,       // Remote CEP-ID will be set during connection
            local_addr,
            remote_addr,
            config,
        );

        self.flows.insert(flow_id, flow);
        flow_id
    }

    /// Gets a mutable reference to a flow
    pub fn get_flow_mut(&mut self, flow_id: u32) -> Option<&mut Flow> {
        self.flows.get_mut(&flow_id)
    }

    /// Gets a reference to a flow
    pub fn get_flow(&self, flow_id: u32) -> Option<&Flow> {
        self.flows.get(&flow_id)
    }

    /// Deallocates a flow
    pub fn deallocate_flow(&mut self, flow_id: u32) -> Result<(), String> {
        self.flows
            .remove(&flow_id)
            .map(|_| ())
            .ok_or_else(|| format!("Flow {} not found", flow_id))
    }

    /// Returns the number of active flows
    pub fn flow_count(&self) -> usize {
        self.flows.len()
    }
}

impl Default for Efcp {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdu::Pdu;

    #[test]
    fn test_flow_send_data() {
        let mut flow = Flow::new(1, 10, 20, 100, 200, FlowConfig::default());

        let payload = vec![0xAA, 0xBB, 0xCC];
        let pdu = flow.send_data(payload.clone()).unwrap();

        assert_eq!(pdu.sequence_num, 0);
        assert_eq!(pdu.payload, payload);
        assert_eq!(flow.next_seq_num, 1);
    }

    #[test]
    fn test_flow_receive_in_order() {
        let mut flow = Flow::new(1, 10, 20, 100, 200, FlowConfig::default());

        let pdu = Pdu::new_data(200, 100, 20, 10, 0, vec![1, 2, 3]);
        let result = flow.receive_pdu(pdu).unwrap();

        assert!(result.is_some());
        assert_eq!(result.unwrap(), vec![1, 2, 3]);
        assert_eq!(flow.expected_seq_num, 1);
    }

    #[test]
    fn test_flow_receive_out_of_order() {
        let mut flow = Flow::new(1, 10, 20, 100, 200, FlowConfig::default());

        // Receive PDU with seq_num 2 (expecting 0)
        let pdu = Pdu::new_data(200, 100, 20, 10, 2, vec![1, 2, 3]);
        let result = flow.receive_pdu(pdu).unwrap();

        // Should buffer it
        assert!(result.is_none());
        assert_eq!(flow.receive_buffer.len(), 1);
    }

    #[test]
    fn test_efcp_flow_allocation() {
        let mut efcp = Efcp::new();

        let flow_id1 = efcp.allocate_flow(100, 200, FlowConfig::default());
        let flow_id2 = efcp.allocate_flow(100, 300, FlowConfig::default());

        assert_eq!(flow_id1, 1);
        assert_eq!(flow_id2, 2);
        assert_eq!(efcp.flow_count(), 2);
    }

    #[test]
    fn test_efcp_flow_deallocation() {
        let mut efcp = Efcp::new();

        let flow_id = efcp.allocate_flow(100, 200, FlowConfig::default());
        assert_eq!(efcp.flow_count(), 1);

        efcp.deallocate_flow(flow_id).unwrap();
        assert_eq!(efcp.flow_count(), 0);
    }

    #[test]
    fn test_ack_handling() {
        let mut flow = Flow::new(1, 10, 20, 100, 200, FlowConfig::default());

        // Send some data
        flow.send_data(vec![1]).unwrap();
        flow.send_data(vec![2]).unwrap();
        assert_eq!(flow.send_window_size(), 2);

        // Receive ACK for seq_num 0
        let ack = Pdu::new_ack(200, 100, 20, 10, 0);
        flow.receive_pdu(ack).unwrap();

        // Window should be reduced
        assert_eq!(flow.send_window_size(), 1);
    }

    #[test]
    fn test_window_full() {
        let config = FlowConfig {
            window_size: 2,
            ..Default::default()
        };
        let mut flow = Flow::new(1, 10, 20, 100, 200, config);

        // Fill the window
        flow.send_data(vec![1]).unwrap();
        flow.send_data(vec![2]).unwrap();

        // Try to send one more - should fail
        let result = flow.send_data(vec![3]);
        assert!(result.is_err());
    }
}
