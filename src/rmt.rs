// SPDX-License-Identifier: EUPL-1.2-or-later
// Copyright © 2026-present RINA (Rust) Contributors

//! Relaying and Multiplexing Task (RMT)
//!
//! RMT is responsible for:
//! - Multiplexing outgoing PDUs from different flows
//! - Demultiplexing incoming PDUs to the correct flow
//! - PDU forwarding based on destination addresses
//! - Queueing and scheduling

use crate::pdu::Pdu;
use std::collections::{HashMap, VecDeque};

/// Forwarding table entry
#[derive(Debug, Clone)]
pub struct ForwardingEntry {
    /// Destination address or prefix
    pub dst_addr: u64,
    /// Next hop address
    pub next_hop: u64,
    /// Cost metric
    pub cost: u32,
}

/// PDU queue for a specific output port/flow
#[derive(Debug)]
struct PduQueue {
    /// Queue of PDUs waiting to be sent
    queue: VecDeque<Pdu>,
    /// Maximum queue size
    max_size: usize,
}

impl PduQueue {
    fn new(max_size: usize) -> Self {
        Self {
            queue: VecDeque::new(),
            max_size,
        }
    }

    fn enqueue(&mut self, pdu: Pdu) -> Result<(), String> {
        if self.queue.len() >= self.max_size {
            return Err("Queue is full".to_string());
        }
        self.queue.push_back(pdu);
        Ok(())
    }

    fn dequeue(&mut self) -> Option<Pdu> {
        self.queue.pop_front()
    }

    fn len(&self) -> usize {
        self.queue.len()
    }

    fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}

/// Relaying and Multiplexing Task
#[derive(Debug)]
pub struct Rmt {
    /// Local address of this IPCP
    local_addr: u64,
    /// Forwarding table: dst_addr -> ForwardingEntry
    forwarding_table: HashMap<u64, ForwardingEntry>,
    /// Output queues for each next hop
    output_queues: HashMap<u64, PduQueue>,
    /// Default queue size
    default_queue_size: usize,
}

impl Rmt {
    /// Creates a new RMT instance
    pub fn new(local_addr: u64) -> Self {
        Self {
            local_addr,
            forwarding_table: HashMap::new(),
            output_queues: HashMap::new(),
            default_queue_size: 100,
        }
    }

    /// Sets the default queue size for output queues
    pub fn set_default_queue_size(&mut self, size: usize) {
        self.default_queue_size = size;
    }

    /// Adds a forwarding table entry
    pub fn add_forwarding_entry(&mut self, entry: ForwardingEntry) {
        let next_hop = entry.next_hop;
        self.forwarding_table.insert(entry.dst_addr, entry);

        // Ensure output queue exists for this next hop
        self.output_queues
            .entry(next_hop)
            .or_insert_with(|| PduQueue::new(self.default_queue_size));
    }

    /// Removes a forwarding table entry
    pub fn remove_forwarding_entry(&mut self, dst_addr: u64) {
        self.forwarding_table.remove(&dst_addr);
    }

    /// Looks up the next hop for a destination address
    pub fn lookup(&self, dst_addr: u64) -> Option<u64> {
        self.forwarding_table
            .get(&dst_addr)
            .map(|entry| entry.next_hop)
    }

    /// Processes an outgoing PDU (from local EFCP)
    ///
    /// Returns the next hop address if forwarding is needed
    pub fn process_outgoing(&mut self, pdu: Pdu) -> Result<u64, String> {
        // Check if this is a local delivery
        if pdu.dst_addr == self.local_addr {
            return Err("PDU destination is local address".to_string());
        }

        // Lookup next hop
        let next_hop = self
            .lookup(pdu.dst_addr)
            .ok_or_else(|| format!("No route to destination {}", pdu.dst_addr))?;

        // Enqueue to output queue
        let queue = self
            .output_queues
            .get_mut(&next_hop)
            .ok_or_else(|| format!("No output queue for next hop {}", next_hop))?;

        queue.enqueue(pdu)?;
        Ok(next_hop)
    }

    /// Processes an incoming PDU (from network/shim)
    ///
    /// Returns:
    /// - Ok(None) if PDU is for local delivery (should go to EFCP)
    /// - Ok(Some(next_hop)) if PDU should be forwarded
    /// - Err if there's an error
    pub fn process_incoming(&mut self, pdu: Pdu) -> Result<Option<u64>, String> {
        // Check if this is for us
        if pdu.dst_addr == self.local_addr {
            // Local delivery - will be handled by EFCP
            return Ok(None);
        }

        // Forward the PDU
        let next_hop = self
            .lookup(pdu.dst_addr)
            .ok_or_else(|| format!("No route to destination {}", pdu.dst_addr))?;

        let queue = self
            .output_queues
            .get_mut(&next_hop)
            .ok_or_else(|| format!("No output queue for next hop {}", next_hop))?;

        queue.enqueue(pdu)?;
        Ok(Some(next_hop))
    }

    /// Dequeues a PDU from the output queue for a specific next hop
    pub fn dequeue_for_next_hop(&mut self, next_hop: u64) -> Option<Pdu> {
        self.output_queues
            .get_mut(&next_hop)
            .and_then(|queue| queue.dequeue())
    }

    /// Returns the queue length for a next hop
    pub fn queue_length(&self, next_hop: u64) -> usize {
        self.output_queues
            .get(&next_hop)
            .map(|queue| queue.len())
            .unwrap_or(0)
    }

    /// Checks if there are any queued PDUs for a next hop
    pub fn has_queued_pdus(&self, next_hop: u64) -> bool {
        self.output_queues
            .get(&next_hop)
            .map(|queue| !queue.is_empty())
            .unwrap_or(false)
    }

    /// Returns the total number of queued PDUs across all queues
    pub fn total_queued(&self) -> usize {
        self.output_queues.values().map(|queue| queue.len()).sum()
    }

    /// Returns the number of forwarding table entries
    pub fn forwarding_table_size(&self) -> usize {
        self.forwarding_table.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdu::{PduType, QoSParameters};

    fn create_test_pdu(src: u64, dst: u64, seq: u64) -> Pdu {
        Pdu {
            src_addr: src,
            dst_addr: dst,
            src_cep_id: 1,
            dst_cep_id: 2,
            sequence_num: seq,
            pdu_type: PduType::Data,
            payload: vec![1, 2, 3],
            qos: QoSParameters::default(),
        }
    }

    #[test]
    fn test_rmt_creation() {
        let rmt = Rmt::new(100);
        assert_eq!(rmt.local_addr, 100);
        assert_eq!(rmt.forwarding_table_size(), 0);
    }

    #[test]
    fn test_add_forwarding_entry() {
        let mut rmt = Rmt::new(100);

        let entry = ForwardingEntry {
            dst_addr: 200,
            next_hop: 150,
            cost: 1,
        };

        rmt.add_forwarding_entry(entry);
        assert_eq!(rmt.forwarding_table_size(), 1);
        assert_eq!(rmt.lookup(200), Some(150));
    }

    #[test]
    fn test_process_outgoing_pdu() {
        let mut rmt = Rmt::new(100);

        // Add forwarding entry
        rmt.add_forwarding_entry(ForwardingEntry {
            dst_addr: 200,
            next_hop: 150,
            cost: 1,
        });

        // Create and process PDU
        let pdu = create_test_pdu(100, 200, 0);
        let result = rmt.process_outgoing(pdu);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 150);
        assert_eq!(rmt.queue_length(150), 1);
    }

    #[test]
    fn test_process_incoming_local_delivery() {
        let mut rmt = Rmt::new(100);

        // PDU destined for local address
        let pdu = create_test_pdu(200, 100, 0);
        let result = rmt.process_incoming(pdu);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None); // Local delivery
    }

    #[test]
    fn test_process_incoming_forward() {
        let mut rmt = Rmt::new(100);

        // Add forwarding entry
        rmt.add_forwarding_entry(ForwardingEntry {
            dst_addr: 300,
            next_hop: 200,
            cost: 1,
        });

        // PDU that needs forwarding
        let pdu = create_test_pdu(50, 300, 0);
        let result = rmt.process_incoming(pdu);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some(200)); // Forward to next hop
        assert_eq!(rmt.queue_length(200), 1);
    }

    #[test]
    fn test_dequeue_pdu() {
        let mut rmt = Rmt::new(100);

        rmt.add_forwarding_entry(ForwardingEntry {
            dst_addr: 200,
            next_hop: 150,
            cost: 1,
        });

        // Enqueue PDU
        let pdu = create_test_pdu(100, 200, 42);
        rmt.process_outgoing(pdu).unwrap();

        // Dequeue it
        let dequeued = rmt.dequeue_for_next_hop(150);
        assert!(dequeued.is_some());
        assert_eq!(dequeued.unwrap().sequence_num, 42);
        assert_eq!(rmt.queue_length(150), 0);
    }

    #[test]
    fn test_no_route() {
        let mut rmt = Rmt::new(100);

        let pdu = create_test_pdu(100, 999, 0);
        let result = rmt.process_outgoing(pdu);

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No route"));
    }

    #[test]
    fn test_queue_full() {
        let mut rmt = Rmt::new(100);
        rmt.set_default_queue_size(2);

        rmt.add_forwarding_entry(ForwardingEntry {
            dst_addr: 200,
            next_hop: 150,
            cost: 1,
        });

        // Fill the queue
        rmt.process_outgoing(create_test_pdu(100, 200, 0)).unwrap();
        rmt.process_outgoing(create_test_pdu(100, 200, 1)).unwrap();

        // Try to add one more
        let result = rmt.process_outgoing(create_test_pdu(100, 200, 2));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("full"));
    }

    #[test]
    fn test_total_queued() {
        let mut rmt = Rmt::new(100);

        rmt.add_forwarding_entry(ForwardingEntry {
            dst_addr: 200,
            next_hop: 150,
            cost: 1,
        });
        rmt.add_forwarding_entry(ForwardingEntry {
            dst_addr: 300,
            next_hop: 250,
            cost: 1,
        });

        rmt.process_outgoing(create_test_pdu(100, 200, 0)).unwrap();
        rmt.process_outgoing(create_test_pdu(100, 200, 1)).unwrap();
        rmt.process_outgoing(create_test_pdu(100, 300, 0)).unwrap();

        assert_eq!(rmt.total_queued(), 3);
    }
}
