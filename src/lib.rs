// SPDX-License-Identifier: EUPL-1.2-or-later
// Copyright © 2026-present ARI Contributors

//! The core library for the ARI implementation.
//!
//! This crate will contain the main logic for implementing the
//! Recursive InterNetwork Architecture, including concepts like
//! DIFs (Distributed IPC Facilities) and IPC Processes.

// Public module declarations
pub mod actors;
pub mod cdap;
pub mod config;
pub mod directory;
pub mod efcp;
pub mod enrollment;
pub mod error;
pub mod fal;
pub mod ipcp;
pub mod pdu;
pub mod policies;
pub mod rib;
pub mod rmt;
pub mod routing;
pub mod shim;

// Re-export commonly used types
pub use actors::{
    EfcpActor, EfcpHandle, EfcpMessage, RibActor, RibHandle, RibMessage, RmtActor, RmtHandle,
    RmtMessage, ShimActor, ShimHandle, ShimMessage,
};
pub use cdap::{CdapMessage, CdapOpCode, CdapSession};
pub use directory::{AddressPool, Directory};
pub use efcp::{Efcp, Flow, FlowConfig};
pub use enrollment::{
    DifConfiguration, EnrollmentManager, EnrollmentRequest, EnrollmentResponse, EnrollmentState,
    NeighborInfo,
};
pub use error::{
    AriError, CdapError, EfcpError, EnrollmentError, RibError, RmtError, SerializationError,
    ShimError,
};
pub use fal::{AllocatedFlow, FlowAllocator, FlowState};
pub use ipcp::{IpcProcess, IpcpState};
pub use pdu::{Pdu, PduType, QoSParameters};
pub use policies::{
    FifoScheduling, PriorityScheduling, QoSPolicy, RoutingPolicy, SchedulingPolicy,
    ShortestPathRouting, SimpleQoSPolicy,
};
pub use rib::{Rib, RibObject, RibValue};
pub use rmt::{ForwardingEntry, Rmt};
pub use routing::{RouteMetadata, RouteResolver, RouteResolverConfig, RouteSnapshot, RouteStats};
pub use shim::{AddressMapper, UdpShim};

/// Represents a Distributed IPC Facility (DIF).
///
/// A DIF is a scope of communication, managed by a set of cooperating
/// IPC Processes, that provides a specific quality of service to its users.
#[derive(Debug)]
pub struct Dif {
    /// Name of this DIF
    pub name: String,
    /// The Resource Information Base for this DIF
    pub rib: Rib,
    /// Directory service for name resolution
    pub directory: Directory,
    /// List of IPCP addresses in this DIF
    pub member_addresses: Vec<u64>,
}

impl Dif {
    /// Creates a new DIF with the given name
    pub fn new_with_name(name: String) -> Self {
        Self {
            name,
            rib: Rib::new(),
            directory: Directory::new(),
            member_addresses: Vec::new(),
        }
    }

    /// Creates a new DIF with a default name
    pub fn new() -> Self {
        Self::new_with_name("default-dif".to_string())
    }

    /// Adds an IPCP to this DIF
    pub fn add_member(&mut self, address: u64) {
        if !self.member_addresses.contains(&address) {
            self.member_addresses.push(address);
        }
    }

    /// Removes an IPCP from this DIF
    pub fn remove_member(&mut self, address: u64) {
        self.member_addresses.retain(|&addr| addr != address);
    }

    /// Returns the number of member IPCPs
    pub fn member_count(&self) -> usize {
        self.member_addresses.len()
    }
}

impl Default for Dif {
    fn default() -> Self {
        Self::new()
    }
}
