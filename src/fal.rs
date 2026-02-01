// SPDX-License-Identifier: EUPL-1.2-or-later
// Copyright © 2026-present ARI Contributors

//! Flow Allocator (FAL)
//!
//! Manages flow allocation and deallocation requests.
//! Handles the flow allocation protocol between IPCPs.

use crate::efcp::FlowConfig;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Flow allocation request
#[derive(Debug, Clone)]
pub struct FlowAllocRequest {
    /// Source application name
    pub src_app_name: String,
    /// Destination application name
    pub dst_app_name: String,
    /// Source address
    pub src_addr: u64,
    /// Destination address
    pub dst_addr: u64,
    /// Requested QoS parameters
    pub qos: FlowConfig,
    /// Request ID
    pub request_id: u64,
}

/// Flow allocation response
#[derive(Debug, Clone)]
pub struct FlowAllocResponse {
    /// Request ID this responds to
    pub request_id: u64,
    /// Whether allocation succeeded
    pub success: bool,
    /// Allocated flow ID (if successful)
    pub flow_id: Option<u32>,
    /// Error message (if failed)
    pub error: Option<String>,
}

/// Flow state
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowState {
    /// Flow allocation pending
    Pending,
    /// Flow is allocated and active
    Allocated,
    /// Flow is being deallocated
    Deallocating,
    /// Flow has been deallocated
    Deallocated,
}

/// Represents an allocated flow
#[derive(Debug, Clone)]
pub struct AllocatedFlow {
    /// Flow ID
    pub flow_id: u32,
    /// Source application name
    pub src_app_name: String,
    /// Destination application name
    pub dst_app_name: String,
    /// Source address
    pub src_addr: u64,
    /// Destination address
    pub dst_addr: u64,
    /// Flow configuration
    pub config: FlowConfig,
    /// Current flow state
    pub state: FlowState,
}

/// Flow Allocator
#[derive(Debug)]
pub struct FlowAllocator {
    /// Allocated flows, keyed by flow ID
    flows: Arc<RwLock<HashMap<u32, AllocatedFlow>>>,
    /// Pending requests, keyed by request ID
    pending_requests: Arc<RwLock<HashMap<u64, FlowAllocRequest>>>,
    /// Next flow ID
    next_flow_id: Arc<RwLock<u32>>,
    /// Next request ID
    next_request_id: Arc<RwLock<u64>>,
}

impl FlowAllocator {
    /// Creates a new flow allocator
    pub fn new() -> Self {
        Self {
            flows: Arc::new(RwLock::new(HashMap::new())),
            pending_requests: Arc::new(RwLock::new(HashMap::new())),
            next_flow_id: Arc::new(RwLock::new(1)),
            next_request_id: Arc::new(RwLock::new(1)),
        }
    }

    /// Creates a flow allocation request
    pub fn create_request(
        &self,
        src_app_name: String,
        dst_app_name: String,
        src_addr: u64,
        dst_addr: u64,
        qos: FlowConfig,
    ) -> FlowAllocRequest {
        let mut request_id_lock = self.next_request_id.write().unwrap();
        let request_id = *request_id_lock;
        *request_id_lock += 1;

        let request = FlowAllocRequest {
            src_app_name,
            dst_app_name,
            src_addr,
            dst_addr,
            qos,
            request_id,
        };

        let mut pending = self.pending_requests.write().unwrap();
        pending.insert(request_id, request.clone());

        request
    }

    /// Processes a flow allocation request and returns a response
    pub fn process_request(&self, request: FlowAllocRequest) -> FlowAllocResponse {
        let mut flow_id_lock = self.next_flow_id.write().unwrap();
        let flow_id = *flow_id_lock;
        *flow_id_lock += 1;

        let allocated_flow = AllocatedFlow {
            flow_id,
            src_app_name: request.src_app_name.clone(),
            dst_app_name: request.dst_app_name.clone(),
            src_addr: request.src_addr,
            dst_addr: request.dst_addr,
            config: request.qos.clone(),
            state: FlowState::Allocated,
        };

        let mut flows = self.flows.write().unwrap();
        flows.insert(flow_id, allocated_flow);

        FlowAllocResponse {
            request_id: request.request_id,
            success: true,
            flow_id: Some(flow_id),
            error: None,
        }
    }

    /// Completes a pending request with a response
    pub fn complete_request(&self, response: FlowAllocResponse) -> Result<(), String> {
        let mut pending = self.pending_requests.write().unwrap();
        pending.remove(&response.request_id);

        if response.success {
            if let Some(flow_id) = response.flow_id {
                let mut flows = self.flows.write().unwrap();
                if let Some(flow) = flows.get_mut(&flow_id) {
                    flow.state = FlowState::Allocated;
                }
            }
            Ok(())
        } else {
            Err(response
                .error
                .unwrap_or_else(|| "Unknown error".to_string()))
        }
    }

    /// Deallocates a flow
    pub fn deallocate_flow(&self, flow_id: u32) -> Result<(), String> {
        let mut flows = self.flows.write().unwrap();

        if let Some(flow) = flows.get_mut(&flow_id) {
            flow.state = FlowState::Deallocated;
            flows.remove(&flow_id);
            Ok(())
        } else {
            Err(format!("Flow {} not found", flow_id))
        }
    }

    /// Gets a flow by ID
    pub fn get_flow(&self, flow_id: u32) -> Option<AllocatedFlow> {
        let flows = self.flows.read().unwrap();
        flows.get(&flow_id).cloned()
    }

    /// Returns the number of allocated flows
    pub fn flow_count(&self) -> usize {
        let flows = self.flows.read().unwrap();
        flows.len()
    }

    /// Returns the number of pending requests
    pub fn pending_count(&self) -> usize {
        let pending = self.pending_requests.read().unwrap();
        pending.len()
    }
}

impl Default for FlowAllocator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fal_create_request() {
        let fal = FlowAllocator::new();

        let request = fal.create_request(
            "app1".to_string(),
            "app2".to_string(),
            1000,
            2000,
            FlowConfig::default(),
        );

        assert_eq!(request.request_id, 1);
        assert_eq!(fal.pending_count(), 1);
    }

    #[test]
    fn test_fal_process_request() {
        let fal = FlowAllocator::new();

        let request = FlowAllocRequest {
            src_app_name: "app1".to_string(),
            dst_app_name: "app2".to_string(),
            src_addr: 1000,
            dst_addr: 2000,
            qos: FlowConfig::default(),
            request_id: 1,
        };

        let response = fal.process_request(request);

        assert!(response.success);
        assert_eq!(response.flow_id, Some(1));
        assert_eq!(fal.flow_count(), 1);
    }

    #[test]
    fn test_fal_deallocate_flow() {
        let fal = FlowAllocator::new();

        let request = FlowAllocRequest {
            src_app_name: "app1".to_string(),
            dst_app_name: "app2".to_string(),
            src_addr: 1000,
            dst_addr: 2000,
            qos: FlowConfig::default(),
            request_id: 1,
        };

        let response = fal.process_request(request);
        let flow_id = response.flow_id.unwrap();

        assert!(fal.deallocate_flow(flow_id).is_ok());
        assert_eq!(fal.flow_count(), 0);
    }

    #[test]
    fn test_fal_get_flow() {
        let fal = FlowAllocator::new();

        let request = FlowAllocRequest {
            src_app_name: "app1".to_string(),
            dst_app_name: "app2".to_string(),
            src_addr: 1000,
            dst_addr: 2000,
            qos: FlowConfig::default(),
            request_id: 1,
        };

        let response = fal.process_request(request);
        let flow_id = response.flow_id.unwrap();

        let flow = fal.get_flow(flow_id);
        assert!(flow.is_some());
        assert_eq!(flow.unwrap().src_app_name, "app1");
    }
}
