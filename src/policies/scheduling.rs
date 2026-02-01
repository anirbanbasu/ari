// SPDX-License-Identifier: EUPL-1.2-or-later
// Copyright © 2026-present ARI Contributors

//! Scheduling Policies
//!
//! Pluggable scheduling algorithms for PDU transmission.

use crate::pdu::Pdu;
use std::collections::VecDeque;

/// Trait for scheduling policies
pub trait SchedulingPolicy: Send + Sync {
    /// Enqueues a PDU
    fn enqueue(&mut self, pdu: Pdu) -> Result<(), String>;
    
    /// Dequeues the next PDU to send
    fn dequeue(&mut self) -> Option<Pdu>;
    
    /// Returns the number of queued PDUs
    fn queue_length(&self) -> usize;
    
    /// Returns the policy name
    fn name(&self) -> &str;
}

/// First-In-First-Out scheduling
#[derive(Debug)]
pub struct FifoScheduling {
    queue: VecDeque<Pdu>,
    max_size: usize,
}

impl FifoScheduling {
    pub fn new(max_size: usize) -> Self {
        Self {
            queue: VecDeque::new(),
            max_size,
        }
    }
}

impl Default for FifoScheduling {
    fn default() -> Self {
        Self::new(1000)
    }
}

impl SchedulingPolicy for FifoScheduling {
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

    fn queue_length(&self) -> usize {
        self.queue.len()
    }

    fn name(&self) -> &str {
        "FIFO"
    }
}

/// Priority-based scheduling
#[derive(Debug)]
pub struct PriorityScheduling {
    /// Separate queues for each priority level
    queues: Vec<VecDeque<Pdu>>,
    max_size_per_queue: usize,
    num_priorities: usize,
}

impl PriorityScheduling {
    pub fn new(num_priorities: usize, max_size_per_queue: usize) -> Self {
        let mut queues = Vec::with_capacity(num_priorities);
        for _ in 0..num_priorities {
            queues.push(VecDeque::new());
        }

        Self {
            queues,
            max_size_per_queue,
            num_priorities,
        }
    }

    fn priority_to_queue_index(&self, priority: u8) -> usize {
        // Map priority (0-255) to queue index
        // Higher priority gets lower index (served first)
        let normalized = priority as usize * self.num_priorities / 256;
        self.num_priorities - 1 - normalized.min(self.num_priorities - 1)
    }
}

impl Default for PriorityScheduling {
    fn default() -> Self {
        Self::new(4, 250) // 4 priority levels, 250 PDUs per queue
    }
}

impl SchedulingPolicy for PriorityScheduling {
    fn enqueue(&mut self, pdu: Pdu) -> Result<(), String> {
        let queue_idx = self.priority_to_queue_index(pdu.qos.priority);
        let queue = &mut self.queues[queue_idx];

        if queue.len() >= self.max_size_per_queue {
            return Err(format!("Priority {} queue is full", queue_idx));
        }

        queue.push_back(pdu);
        Ok(())
    }

    fn dequeue(&mut self) -> Option<Pdu> {
        // Serve highest priority (lowest index) first
        for queue in &mut self.queues {
            if let Some(pdu) = queue.pop_front() {
                return Some(pdu);
            }
        }
        None
    }

    fn queue_length(&self) -> usize {
        self.queues.iter().map(|q| q.len()).sum()
    }

    fn name(&self) -> &str {
        "Priority"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdu::QoSParameters;

    #[test]
    fn test_fifo_scheduling() {
        let mut sched = FifoScheduling::new(10);
        
        let pdu1 = Pdu::new_data(1, 2, 1, 2, 0, vec![1]);
        let pdu2 = Pdu::new_data(1, 2, 1, 2, 1, vec![2]);
        
        sched.enqueue(pdu1.clone()).unwrap();
        sched.enqueue(pdu2.clone()).unwrap();
        
        assert_eq!(sched.queue_length(), 2);
        
        let dequeued = sched.dequeue().unwrap();
        assert_eq!(dequeued.sequence_num, 0); // FIFO order
    }

    #[test]
    fn test_priority_scheduling() {
        let mut sched = PriorityScheduling::new(4, 10);
        
        let low_pri = Pdu::new_data_with_qos(
            1, 2, 1, 2, 0, vec![1],
            QoSParameters { priority: 50, ..Default::default() }
        );
        
        let high_pri = Pdu::new_data_with_qos(
            1, 2, 1, 2, 1, vec![2],
            QoSParameters { priority: 200, ..Default::default() }
        );
        
        sched.enqueue(low_pri).unwrap();
        sched.enqueue(high_pri).unwrap();
        
        // High priority should be dequeued first
        let dequeued = sched.dequeue().unwrap();
        assert_eq!(dequeued.sequence_num, 1);
    }

    #[test]
    fn test_scheduling_full_queue() {
        let mut sched = FifoScheduling::new(2);
        
        let pdu = Pdu::new_data(1, 2, 1, 2, 0, vec![1]);
        
        sched.enqueue(pdu.clone()).unwrap();
        sched.enqueue(pdu.clone()).unwrap();
        
        let result = sched.enqueue(pdu);
        assert!(result.is_err());
    }
}
