// SPDX-License-Identifier: EUPL-1.2-or-later
// Copyright © 2026-present ARI Contributors

//! Inter-IPCP Flow Allocator
//!
//! Manages connectivity between IPCPs (N-1 flows in RINA terminology).
//! This provides an abstraction layer between routing decisions (RMT) and
//! underlay transport (Shim), handling flow lifecycle and connection state.

use crate::pdu::Pdu;
use crate::rib::Rib;
use crate::shim::Shim;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// State of an Inter-IPCP flow
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterIpcpFlowState {
    /// Flow is active and operational
    Active,
    /// Flow has not been used recently (candidate for cleanup)
    Stale,
    /// Flow has failed and needs recovery
    Failed,
}

/// Represents a bidirectional connection to a neighboring IPCP
#[derive(Debug)]
pub struct InterIpcpFlow {
    /// Remote RINA address (next-hop neighbor)
    pub remote_addr: u64,

    /// Socket address for the remote peer
    pub socket_addr: SocketAddr,

    /// Current state of the flow
    pub state: InterIpcpFlowState,

    /// Last time this flow was used
    pub last_activity: Instant,

    /// Statistics
    pub sent_pdus: u64,
    pub received_pdus: u64,
    pub send_errors: u64,
}

impl InterIpcpFlow {
    /// Creates a new Inter-IPCP flow
    pub fn new(remote_addr: u64, socket_addr: SocketAddr) -> Self {
        Self {
            remote_addr,
            socket_addr,
            state: InterIpcpFlowState::Active,
            last_activity: Instant::now(),
            sent_pdus: 0,
            received_pdus: 0,
            send_errors: 0,
        }
    }

    /// Updates the socket address (e.g., after DHCP renewal)
    pub fn update_address(&mut self, new_socket_addr: SocketAddr) {
        self.socket_addr = new_socket_addr;
        self.last_activity = Instant::now();
        self.state = InterIpcpFlowState::Active;
    }

    /// Records successful PDU transmission
    pub fn record_send(&mut self) {
        self.sent_pdus += 1;
        self.last_activity = Instant::now();
        self.state = InterIpcpFlowState::Active;
    }

    /// Records send failure
    pub fn record_send_error(&mut self) {
        self.send_errors += 1;
        self.state = InterIpcpFlowState::Failed;
    }

    /// Records PDU reception
    pub fn record_receive(&mut self) {
        self.received_pdus += 1;
        self.last_activity = Instant::now();
        self.state = InterIpcpFlowState::Active;
    }

    /// Checks if flow is stale (no activity for duration)
    pub fn is_stale(&self, timeout: Duration) -> bool {
        self.last_activity.elapsed() > timeout
    }
}

/// Inter-IPCP Flow Allocator
///
/// Manages bidirectional flows between this IPCP and its neighbors.
/// Provides an abstraction layer between RMT (routing) and Shim (transport).
pub struct InterIpcpFlowAllocator {
    /// Active flows to neighbors, keyed by remote RINA address
    flows: Arc<Mutex<HashMap<u64, InterIpcpFlow>>>,

    /// Reference to RIB for route lookups
    rib: Rib,

    /// Shim layer for actual transport
    shim: Arc<dyn Shim>,

    /// Timeout for marking flows as stale
    stale_timeout: Duration,
}

impl InterIpcpFlowAllocator {
    /// Creates a new Inter-IPCP Flow Allocator
    pub fn new(rib: Rib, shim: Arc<dyn Shim>) -> Self {
        Self {
            flows: Arc::new(Mutex::new(HashMap::new())),
            rib,
            shim,
            stale_timeout: Duration::from_secs(300), // 5 minutes default
        }
    }

    /// Sets the timeout for marking flows as stale
    pub fn set_stale_timeout(&mut self, timeout: Duration) {
        self.stale_timeout = timeout;
    }

    /// Gets or creates a flow to the specified neighbor
    ///
    /// This is the main entry point for RMT to obtain connectivity.
    /// If no flow exists, it will be created lazily by looking up the
    /// route in the RIB.
    pub async fn get_or_create_flow(&self, remote_addr: u64) -> Result<(), String> {
        // Check if flow already exists
        {
            let flows = self.flows.lock().unwrap();
            if let Some(flow) = flows.get(&remote_addr)
                && flow.state == InterIpcpFlowState::Active
            {
                return Ok(());
            }
        } // Lock is dropped here before await

        // Need to create new flow - lookup route in RIB
        let socket_addr = self.lookup_route(remote_addr).await?;

        // Register peer mapping in shim
        self.shim.register_peer(remote_addr, socket_addr);

        // Create and store the flow
        {
            let mut flows = self.flows.lock().unwrap();
            let flow = InterIpcpFlow::new(remote_addr, socket_addr);
            flows.insert(remote_addr, flow);
        }

        Ok(())
    }

    /// Sends a PDU over the Inter-IPCP flow to the specified neighbor
    pub fn send_pdu(&self, next_hop: u64, pdu: &Pdu) -> Result<(), String> {
        // Update flow statistics
        {
            let mut flows = self.flows.lock().unwrap();
            if let Some(flow) = flows.get_mut(&next_hop) {
                flow.record_send();
            }
        }

        // Send via shim
        self.shim.send_pdu(pdu).map_err(|e| {
            // Record error
            let mut flows = self.flows.lock().unwrap();
            if let Some(flow) = flows.get_mut(&next_hop) {
                flow.record_send_error();
            }
            format!("Failed to send PDU to {}: {}", next_hop, e)
        })?;

        Ok(())
    }

    /// Updates the socket address for a neighbor
    ///
    /// Called when a peer's underlay address changes (e.g., DHCP renewal).
    pub fn update_peer_address(&self, remote_addr: u64, new_socket_addr: SocketAddr) {
        let mut flows = self.flows.lock().unwrap();

        if let Some(flow) = flows.get_mut(&remote_addr) {
            flow.update_address(new_socket_addr);
        } else {
            // Create new flow with the address
            let flow = InterIpcpFlow::new(remote_addr, new_socket_addr);
            flows.insert(remote_addr, flow);
        }

        // Update shim mapping
        self.shim.register_peer(remote_addr, new_socket_addr);
    }

    /// Records reception of a PDU from a neighbor
    ///
    /// Updates flow statistics and potentially creates flow if not seen before.
    pub fn record_received_from(&self, remote_addr: u64, socket_addr: SocketAddr) {
        let mut flows = self.flows.lock().unwrap();

        if let Some(flow) = flows.get_mut(&remote_addr) {
            flow.record_receive();

            // Update address if it changed
            if flow.socket_addr != socket_addr {
                flow.update_address(socket_addr);
                self.shim.register_peer(remote_addr, socket_addr);
            }
        } else {
            // First time seeing this peer - create flow
            let mut flow = InterIpcpFlow::new(remote_addr, socket_addr);
            flow.record_receive();
            flows.insert(remote_addr, flow);
            self.shim.register_peer(remote_addr, socket_addr);
        }
    }

    /// Cleans up stale flows
    ///
    /// Should be called periodically to remove inactive flows.
    pub fn cleanup_stale_flows(&self) -> usize {
        let mut flows = self.flows.lock().unwrap();
        let initial_count = flows.len();

        flows.retain(|_, flow| !flow.is_stale(self.stale_timeout));

        initial_count - flows.len()
    }

    /// Gets statistics for all flows
    pub fn get_flow_stats(&self) -> Vec<(u64, InterIpcpFlowState, u64, u64)> {
        let flows = self.flows.lock().unwrap();
        flows
            .iter()
            .map(|(addr, flow)| (*addr, flow.state, flow.sent_pdus, flow.received_pdus))
            .collect()
    }

    /// Gets the number of active flows
    pub fn active_flow_count(&self) -> usize {
        let flows = self.flows.lock().unwrap();
        flows
            .values()
            .filter(|f| f.state == InterIpcpFlowState::Active)
            .count()
    }

    /// Explicitly closes a flow
    pub fn close_flow(&self, remote_addr: u64) -> bool {
        let mut flows = self.flows.lock().unwrap();
        flows.remove(&remote_addr).is_some()
    }

    /// Lookup route in RIB to get socket address for a remote RINA address
    async fn lookup_route(&self, remote_addr: u64) -> Result<SocketAddr, String> {
        // Try dynamic routes first
        let route_name = format!("/routing/dynamic/{}", remote_addr);
        if let Some(route_obj) = self.rib.read(&route_name).await
            && let crate::rib::RibValue::Struct(route_struct) = &route_obj.value
            && let Some(next_hop_box) = route_struct.get("next_hop_address")
            && let Some(addr_str) = next_hop_box.as_string()
        {
            return addr_str
                .parse::<SocketAddr>()
                .map_err(|e| format!("Invalid socket address: {}", e));
        }

        // Try static routes as fallback
        let static_route_name = format!("/routing/static/{}", remote_addr);
        if let Some(route_obj) = self.rib.read(&static_route_name).await
            && let crate::rib::RibValue::Struct(route_struct) = &route_obj.value
            && let Some(next_hop_box) = route_struct.get("next_hop_address")
            && let Some(addr_str) = next_hop_box.as_string()
        {
            return addr_str
                .parse::<SocketAddr>()
                .map_err(|e| format!("Invalid socket address: {}", e));
        }

        Err(format!("No route found for RINA address {}", remote_addr))
    }
}

impl std::fmt::Debug for InterIpcpFlowAllocator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let flows = self.flows.lock().unwrap();
        f.debug_struct("InterIpcpFlowAllocator")
            .field("flow_count", &flows.len())
            .field("stale_timeout", &self.stale_timeout)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shim::UdpShim;
    use std::thread;

    #[tokio::test]
    async fn test_inter_ipcp_flow_creation() {
        let flow = InterIpcpFlow::new(1002, "127.0.0.1:7001".parse().unwrap());

        assert_eq!(flow.remote_addr, 1002);
        assert_eq!(flow.state, InterIpcpFlowState::Active);
        assert_eq!(flow.sent_pdus, 0);
        assert_eq!(flow.received_pdus, 0);
    }

    #[tokio::test]
    async fn test_inter_ipcp_flow_statistics() {
        let mut flow = InterIpcpFlow::new(1002, "127.0.0.1:7001".parse().unwrap());

        flow.record_send();
        assert_eq!(flow.sent_pdus, 1);

        flow.record_receive();
        assert_eq!(flow.received_pdus, 1);

        flow.record_send_error();
        assert_eq!(flow.send_errors, 1);
        assert_eq!(flow.state, InterIpcpFlowState::Failed);
    }

    #[tokio::test]
    async fn test_inter_ipcp_flow_stale_detection() {
        let mut flow = InterIpcpFlow::new(1002, "127.0.0.1:7001".parse().unwrap());

        // Flow should not be stale immediately
        assert!(!flow.is_stale(Duration::from_millis(100)));

        // Wait and check
        thread::sleep(Duration::from_millis(150));
        assert!(flow.is_stale(Duration::from_millis(100)));

        // Activity resets staleness
        flow.record_send();
        assert!(!flow.is_stale(Duration::from_millis(100)));
    }

    #[tokio::test]
    async fn test_inter_ipcp_flow_address_update() {
        let mut flow = InterIpcpFlow::new(1002, "127.0.0.1:7001".parse().unwrap());

        let new_addr: SocketAddr = "127.0.0.1:7002".parse().unwrap();
        flow.update_address(new_addr);

        assert_eq!(flow.socket_addr, new_addr);
        assert_eq!(flow.state, InterIpcpFlowState::Active);
    }

    #[tokio::test]
    async fn test_flow_allocator_creation() {
        let rib = Rib::new();
        let shim = Arc::new(UdpShim::new(1001));
        let fal = InterIpcpFlowAllocator::new(rib, shim);

        assert_eq!(fal.active_flow_count(), 0);
    }

    #[tokio::test]
    async fn test_flow_allocator_peer_tracking() {
        let rib = Rib::new();
        let shim = Arc::new(UdpShim::new(1001));
        let fal = InterIpcpFlowAllocator::new(rib, shim);

        let socket_addr: SocketAddr = "127.0.0.1:7001".parse().unwrap();
        fal.record_received_from(1002, socket_addr);

        assert_eq!(fal.active_flow_count(), 1);
    }

    #[tokio::test]
    async fn test_flow_allocator_address_update() {
        let rib = Rib::new();
        let shim = Arc::new(UdpShim::new(1001));
        let fal = InterIpcpFlowAllocator::new(rib, shim);

        let old_addr: SocketAddr = "127.0.0.1:7001".parse().unwrap();
        let new_addr: SocketAddr = "127.0.0.1:7002".parse().unwrap();

        fal.update_peer_address(1002, old_addr);
        assert_eq!(fal.active_flow_count(), 1);

        fal.update_peer_address(1002, new_addr);
        assert_eq!(fal.active_flow_count(), 1); // Still 1 flow, just updated
    }

    #[tokio::test]
    async fn test_flow_allocator_cleanup() {
        let rib = Rib::new();
        let shim = Arc::new(UdpShim::new(1001));
        let mut fal = InterIpcpFlowAllocator::new(rib, shim);
        fal.set_stale_timeout(Duration::from_millis(100));

        let socket_addr: SocketAddr = "127.0.0.1:7001".parse().unwrap();
        fal.record_received_from(1002, socket_addr);

        assert_eq!(fal.active_flow_count(), 1);

        // Wait for flow to become stale
        thread::sleep(Duration::from_millis(150));

        let cleaned = fal.cleanup_stale_flows();
        assert_eq!(cleaned, 1);
        assert_eq!(fal.active_flow_count(), 0);
    }
}
