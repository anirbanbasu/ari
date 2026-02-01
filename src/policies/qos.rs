// SPDX-License-Identifier: EUPL-1.2-or-later
// Copyright © 2026-present ARI Contributors

//! QoS Policies
//!
//! Quality of Service management policies.

use crate::pdu::{Pdu, QoSParameters};

/// Trait for QoS policies
pub trait QoSPolicy: Send + Sync {
    /// Checks if a PDU meets QoS requirements
    fn check_qos(&self, pdu: &Pdu) -> bool;

    /// Applies QoS parameters to a PDU
    fn apply_qos(&self, pdu: &mut Pdu, qos: QoSParameters);

    /// Determines if a PDU should be dropped due to QoS constraints
    fn should_drop(&self, pdu: &Pdu, queue_length: usize) -> bool;

    /// Returns the policy name
    fn name(&self) -> &str;
}

/// Simple QoS policy implementation
#[derive(Debug)]
pub struct SimpleQoSPolicy {
    /// Maximum queue length before dropping low priority packets
    max_queue_length: usize,
}

impl SimpleQoSPolicy {
    pub fn new(max_queue_length: usize) -> Self {
        Self { max_queue_length }
    }
}

impl Default for SimpleQoSPolicy {
    fn default() -> Self {
        Self::new(1000)
    }
}

impl QoSPolicy for SimpleQoSPolicy {
    fn check_qos(&self, pdu: &Pdu) -> bool {
        // Check basic QoS parameters
        if let Some(max_delay) = pdu.qos.max_delay_ms
            && max_delay == 0
        {
            return false;
        }
        true
    }

    fn apply_qos(&self, pdu: &mut Pdu, qos: QoSParameters) {
        pdu.qos = qos;
    }

    fn should_drop(&self, pdu: &Pdu, queue_length: usize) -> bool {
        // Drop low priority packets when queue is getting full
        if queue_length > self.max_queue_length * 3 / 4 {
            // Drop anything below medium priority
            return pdu.qos.priority < 128;
        }

        // Drop everything when completely full
        queue_length >= self.max_queue_length
    }

    fn name(&self) -> &str {
        "SimpleQoS"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qos_check() {
        let policy = SimpleQoSPolicy::default();
        let pdu = Pdu::new_data(1, 2, 1, 2, 0, vec![1]);

        assert!(policy.check_qos(&pdu));
    }

    #[test]
    fn test_qos_apply() {
        let policy = SimpleQoSPolicy::default();
        let mut pdu = Pdu::new_data(1, 2, 1, 2, 0, vec![1]);

        let qos = QoSParameters {
            priority: 200,
            max_delay_ms: Some(100),
            ..Default::default()
        };

        policy.apply_qos(&mut pdu, qos);
        assert_eq!(pdu.qos.priority, 200);
    }

    #[test]
    fn test_qos_should_drop() {
        let policy = SimpleQoSPolicy::new(100);

        let low_pri = Pdu::new_data_with_qos(
            1,
            2,
            1,
            2,
            0,
            vec![1],
            QoSParameters {
                priority: 50,
                ..Default::default()
            },
        );

        // Should drop when queue is 75% full
        assert!(policy.should_drop(&low_pri, 76));

        let high_pri = Pdu::new_data_with_qos(
            1,
            2,
            1,
            2,
            0,
            vec![1],
            QoSParameters {
                priority: 200,
                ..Default::default()
            },
        );

        // Should not drop high priority at 75%
        assert!(!policy.should_drop(&high_pri, 76));
    }
}
