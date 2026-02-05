// SPDX-License-Identifier: EUPL-1.2-or-later
// Copyright © 2026-present ARI Contributors

//! IPC Process (IPCP) Management
//!
//! Manages IPCP lifecycle, state, and component coordination.

use crate::cdap::CdapSession;
use crate::directory::Directory;
use crate::efcp::Efcp;
use crate::enrollment::{EnrollmentManager, EnrollmentState};
use crate::fal::FlowAllocator;
use crate::rib::Rib;
use crate::rmt::Rmt;
use crate::shim::UdpShim;
use std::sync::Arc;

/// IPCP operational state
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IpcpState {
    /// IPCP is initializing
    Initializing,
    /// IPCP is ready but not enrolled in a DIF
    Ready,
    /// IPCP is enrolling in a DIF
    Enrolling,
    /// IPCP is operational and enrolled
    Operational,
    /// IPCP is shutting down
    ShuttingDown,
    /// IPCP has shut down
    Shutdown,
    /// IPCP is in error state
    Error(String),
}

/// Complete IPC Process with all components
#[derive(Debug)]
pub struct IpcProcess {
    /// The name of this IPC Process
    pub name: Option<String>,
    /// The address of this IPC Process within its DIF
    pub address: Option<u64>,
    /// Name of the DIF this IPCP belongs to
    pub dif_name: Option<String>,
    /// Current operational state
    pub state: IpcpState,
    /// The Resource Information Base for this IPCP
    pub rib: Rib,
    /// CDAP session for distributed operations
    pub cdap: CdapSession,
    /// EFCP instance for flow management
    pub efcp: Efcp,
    /// RMT for PDU forwarding
    pub rmt: Rmt,
    /// Shim layer for network communication
    pub shim: UdpShim,
    /// Flow allocator
    pub fal: FlowAllocator,
    /// Directory service
    pub directory: Directory,
    /// Enrollment manager
    pub enrollment: EnrollmentManager,
}

impl IpcProcess {
    /// Creates a new IPC Process with an empty RIB
    pub fn new() -> Self {
        let rib = Rib::new();
        let address = 0;
        let shim = UdpShim::new(address);
        let shim_for_enrollment = Arc::new(UdpShim::new(address));

        Self {
            cdap: CdapSession::new(rib.clone()),
            enrollment: EnrollmentManager::new(rib.clone(), shim_for_enrollment),
            rib,
            name: None,
            address: None,
            dif_name: None,
            state: IpcpState::Initializing,
            efcp: Efcp::new(),
            rmt: Rmt::new(address),
            shim,
            fal: FlowAllocator::new(),
            directory: Directory::new(),
        }
    }

    /// Creates a new IPC Process with a given name and address
    pub fn with_name_and_address(name: String, address: u64) -> Self {
        let rib = Rib::new();
        let shim = UdpShim::new(address);
        let shim_for_enrollment = Arc::new(UdpShim::new(address));

        Self {
            cdap: CdapSession::new(rib.clone()),
            enrollment: EnrollmentManager::new(rib.clone(), shim_for_enrollment),
            rib,
            name: Some(name),
            address: Some(address),
            dif_name: None,
            state: IpcpState::Ready,
            efcp: Efcp::new(),
            rmt: Rmt::new(address),
            shim,
            fal: FlowAllocator::new(),
            directory: Directory::new(),
        }
    }

    /// Creates a new IPC Process with a given name
    pub fn with_name(name: String) -> Self {
        Self::with_name_and_address(name, 0)
    }

    /// Sets the address for this IPC Process
    pub fn set_address(&mut self, address: u64) {
        self.address = Some(address);
        self.rmt = Rmt::new(address);
        self.shim = UdpShim::new(address);
    }

    /// Sets the DIF name
    pub fn set_dif_name(&mut self, dif_name: String) {
        self.dif_name = Some(dif_name);
    }

    /// Transitions to a new state
    pub fn set_state(&mut self, state: IpcpState) {
        self.state = state;
    }

    /// Checks if IPCP is operational
    pub fn is_operational(&self) -> bool {
        self.state == IpcpState::Operational
    }

    /// Checks if IPCP is enrolled
    pub fn is_enrolled(&self) -> bool {
        *self.enrollment.state() == EnrollmentState::Enrolled
    }

    /// Starts the IPCP
    pub fn start(&mut self) -> Result<(), String> {
        if self.state == IpcpState::Shutdown {
            return Err("Cannot start a shutdown IPCP".to_string());
        }

        self.state = IpcpState::Ready;
        Ok(())
    }

    /// Shuts down the IPCP
    pub fn shutdown(&mut self) -> Result<(), String> {
        self.state = IpcpState::ShuttingDown;

        // TODO: Clean up resources
        // - Deallocate all flows
        // - Close shim connections
        // - Clear RIB

        self.state = IpcpState::Shutdown;
        Ok(())
    }
}

impl Default for IpcProcess {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ipcp_creation() {
        let ipcp = IpcProcess::new();
        assert_eq!(ipcp.state, IpcpState::Initializing);
        assert!(ipcp.name.is_none());
    }

    #[test]
    fn test_ipcp_with_name_and_address() {
        let ipcp = IpcProcess::with_name_and_address("test-ipcp".to_string(), 1000);
        assert_eq!(ipcp.name, Some("test-ipcp".to_string()));
        assert_eq!(ipcp.address, Some(1000));
        assert_eq!(ipcp.state, IpcpState::Ready);
    }

    #[test]
    fn test_ipcp_set_address() {
        let mut ipcp = IpcProcess::new();
        ipcp.set_address(2000);
        assert_eq!(ipcp.address, Some(2000));
    }

    #[test]
    fn test_ipcp_state_transitions() {
        let mut ipcp = IpcProcess::new();

        ipcp.start().unwrap();
        assert_eq!(ipcp.state, IpcpState::Ready);

        ipcp.set_state(IpcpState::Operational);
        assert!(ipcp.is_operational());

        ipcp.shutdown().unwrap();
        assert_eq!(ipcp.state, IpcpState::Shutdown);
    }

    #[test]
    fn test_ipcp_cannot_start_after_shutdown() {
        let mut ipcp = IpcProcess::new();
        ipcp.shutdown().unwrap();

        let result = ipcp.start();
        assert!(result.is_err());
    }

    #[test]
    fn test_ipcp_dif_name() {
        let mut ipcp = IpcProcess::new();
        ipcp.set_dif_name("test-dif".to_string());
        assert_eq!(ipcp.dif_name, Some("test-dif".to_string()));
    }
}
