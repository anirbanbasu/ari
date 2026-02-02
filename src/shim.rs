// SPDX-License-Identifier: EUPL-1.2-or-later
// Copyright © 2026-present ARI Contributors

//! Shim Layer - UDP/IP abstraction
//!
//! This module provides a shim layer that abstracts away the UDP/IP
//! networking details, allowing RINA to operate over standard IP networks.
//! It handles socket management, address translation, and packet I/O.

use crate::pdu::Pdu;
use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Shim layer error types
#[derive(Debug)]
pub enum ShimError {
    /// Socket binding error
    BindError(String),
    /// Send error
    SendError(String),
    /// Receive error
    ReceiveError(String),
    /// Address parsing error
    AddressError(String),
    /// Socket not bound
    NotBound,
}

impl std::fmt::Display for ShimError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShimError::BindError(msg) => write!(f, "Bind error: {}", msg),
            ShimError::SendError(msg) => write!(f, "Send error: {}", msg),
            ShimError::ReceiveError(msg) => write!(f, "Receive error: {}", msg),
            ShimError::AddressError(msg) => write!(f, "Address error: {}", msg),
            ShimError::NotBound => write!(f, "Socket not bound"),
        }
    }
}

impl std::error::Error for ShimError {}

impl From<ShimError> for String {
    fn from(err: ShimError) -> String {
        err.to_string()
    }
}

/// Maps RINA addresses to UDP socket addresses
#[derive(Debug, Clone)]
pub struct AddressMapping {
    /// RINA address
    pub rina_addr: u64,
    /// Corresponding UDP socket address
    pub socket_addr: SocketAddr,
}

/// UDP/IP Shim Layer
///
/// Provides abstraction over UDP sockets for RINA communication
pub struct UdpShim {
    /// The underlying UDP socket
    socket: Arc<Mutex<Option<UdpSocket>>>,
    /// Local RINA address
    local_rina_addr: u64,
    /// Maximum receive buffer size
    max_buffer_size: usize,
    /// Address mapper for RINA to socket address translation
    address_mapper: Arc<Mutex<HashMap<u64, SocketAddr>>>,
}

impl UdpShim {
    /// Creates a new UDP shim layer
    pub fn new(local_rina_addr: u64) -> Self {
        Self {
            socket: Arc::new(Mutex::new(None)),
            local_rina_addr,
            max_buffer_size: 65536,
            address_mapper: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Binds the shim to a UDP socket address
    pub fn bind(&self, addr: &str) -> Result<(), ShimError> {
        let socket = UdpSocket::bind(addr)
            .map_err(|e| ShimError::BindError(format!("Failed to bind to {}: {}", addr, e)))?;

        // Set non-blocking mode with a timeout
        socket
            .set_read_timeout(Some(Duration::from_millis(100)))
            .map_err(|e| ShimError::BindError(format!("Failed to set read timeout: {}", e)))?;

        let mut sock_guard = self.socket.lock().unwrap();
        *sock_guard = Some(socket);

        Ok(())
    }

    /// Sends data to a destination UDP address
    pub fn send_to(&self, data: &[u8], dest_addr: &str) -> Result<usize, ShimError> {
        let sock_guard = self.socket.lock().unwrap();
        let socket = sock_guard.as_ref().ok_or(ShimError::NotBound)?;

        let dest: SocketAddr = dest_addr.parse().map_err(|e| {
            ShimError::AddressError(format!("Invalid address {}: {}", dest_addr, e))
        })?;

        socket
            .send_to(data, dest)
            .map_err(|e| ShimError::SendError(format!("Failed to send: {}", e)))
    }

    /// Receives data from the socket
    ///
    /// Returns (data, source_address) if data was received,
    /// or None if no data is available (non-blocking)
    pub fn recv_from(&self) -> Result<Option<(Vec<u8>, SocketAddr)>, ShimError> {
        let sock_guard = self.socket.lock().unwrap();
        let socket = sock_guard.as_ref().ok_or(ShimError::NotBound)?;

        let mut buffer = vec![0u8; self.max_buffer_size];

        match socket.recv_from(&mut buffer) {
            Ok((size, src_addr)) => {
                buffer.truncate(size);
                Ok(Some((buffer, src_addr)))
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // No data available (timeout)
                Ok(None)
            }
            Err(e) => Err(ShimError::ReceiveError(format!("Failed to receive: {}", e))),
        }
    }

    /// Returns the local socket address if bound
    pub fn local_addr(&self) -> Result<SocketAddr, ShimError> {
        let sock_guard = self.socket.lock().unwrap();
        let socket = sock_guard.as_ref().ok_or(ShimError::NotBound)?;

        socket
            .local_addr()
            .map_err(|e| ShimError::ReceiveError(format!("Failed to get local address: {}", e)))
    }

    /// Returns the local RINA address
    pub fn local_rina_addr(&self) -> u64 {
        self.local_rina_addr
    }

    /// Sets the maximum receive buffer size
    pub fn set_max_buffer_size(&mut self, size: usize) {
        self.max_buffer_size = size;
    }

    /// Registers a RINA address to socket address mapping
    pub fn register_peer(&self, rina_addr: u64, socket_addr: SocketAddr) {
        let mut mapper = self.address_mapper.lock().unwrap();
        mapper.insert(rina_addr, socket_addr);
    }

    /// Looks up socket address for a RINA address
    pub fn lookup_peer(&self, rina_addr: u64) -> Option<SocketAddr> {
        let mapper = self.address_mapper.lock().unwrap();
        mapper.get(&rina_addr).copied()
    }

    /// Sends a PDU over the network
    pub fn send_pdu(&self, pdu: &Pdu) -> Result<usize, ShimError> {
        // Serialize the PDU
        let data = pdu
            .serialize()
            .map_err(|e| ShimError::SendError(format!("PDU serialization failed: {}", e)))?;

        // Look up destination socket address
        let dest_socket = self.lookup_peer(pdu.dst_addr).ok_or_else(|| {
            ShimError::SendError(format!(
                "No mapping found for RINA address {}",
                pdu.dst_addr
            ))
        })?;

        // Send via UDP
        self.send_to(&data, &dest_socket.to_string())
    }

    /// Receives a PDU from the network
    /// Returns the PDU and the source socket address it was received from
    pub fn receive_pdu(&self) -> Result<Option<(Pdu, SocketAddr)>, ShimError> {
        // Receive raw data
        let result = self.recv_from()?;

        match result {
            Some((data, src_addr)) => {
                // Deserialize PDU
                let pdu = Pdu::deserialize(&data).map_err(|e| {
                    ShimError::ReceiveError(format!("PDU deserialization failed: {}", e))
                })?;

                Ok(Some((pdu, src_addr)))
            }
            None => Ok(None),
        }
    }
}

impl std::fmt::Debug for UdpShim {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UdpShim")
            .field("local_rina_addr", &self.local_rina_addr)
            .field("max_buffer_size", &self.max_buffer_size)
            .field("bound", &self.socket.lock().unwrap().is_some())
            .finish()
    }
}

/// Simple address mapper for RINA to UDP/IP translation
pub struct AddressMapper {
    /// Mapping from RINA address to socket address
    mappings: Mutex<std::collections::HashMap<u64, SocketAddr>>,
}

impl AddressMapper {
    /// Creates a new address mapper
    pub fn new() -> Self {
        Self {
            mappings: Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// Adds a mapping
    pub fn add_mapping(&self, rina_addr: u64, socket_addr: SocketAddr) {
        let mut mappings = self.mappings.lock().unwrap();
        mappings.insert(rina_addr, socket_addr);
    }

    /// Looks up a socket address for a RINA address
    pub fn lookup(&self, rina_addr: u64) -> Option<SocketAddr> {
        let mappings = self.mappings.lock().unwrap();
        mappings.get(&rina_addr).copied()
    }

    /// Removes a mapping
    pub fn remove_mapping(&self, rina_addr: u64) {
        let mut mappings = self.mappings.lock().unwrap();
        mappings.remove(&rina_addr);
    }

    /// Returns the number of mappings
    pub fn mapping_count(&self) -> usize {
        let mappings = self.mappings.lock().unwrap();
        mappings.len()
    }
}

impl Default for AddressMapper {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shim_creation() {
        let shim = UdpShim::new(1000);
        assert_eq!(shim.local_rina_addr(), 1000);
    }

    #[test]
    fn test_shim_bind() {
        let shim = UdpShim::new(1000);
        let result = shim.bind("127.0.0.1:0"); // Bind to any available port
        assert!(result.is_ok());

        let local_addr = shim.local_addr();
        assert!(local_addr.is_ok());
    }

    #[test]
    fn test_shim_send_receive() {
        let shim1 = UdpShim::new(1000);
        let shim2 = UdpShim::new(2000);

        // Bind both shims
        shim1.bind("127.0.0.1:0").unwrap();
        shim2.bind("127.0.0.1:0").unwrap();

        let addr1 = shim1.local_addr().unwrap();
        let addr2 = shim2.local_addr().unwrap();

        // Send from shim1 to shim2
        let test_data = b"Hello, RINA!";
        let sent = shim1.send_to(test_data, &addr2.to_string()).unwrap();
        assert_eq!(sent, test_data.len());

        // Receive on shim2
        std::thread::sleep(Duration::from_millis(50));
        let received = shim2.recv_from().unwrap();
        assert!(received.is_some());

        let (data, src) = received.unwrap();
        assert_eq!(&data, test_data);
        assert_eq!(src, addr1);
    }

    #[test]
    fn test_shim_recv_timeout() {
        let shim = UdpShim::new(1000);
        shim.bind("127.0.0.1:0").unwrap();

        // Try to receive when no data is available
        let result = shim.recv_from().unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_address_mapper() {
        let mapper = AddressMapper::new();

        let socket_addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        mapper.add_mapping(1000, socket_addr);

        assert_eq!(mapper.mapping_count(), 1);
        assert_eq!(mapper.lookup(1000), Some(socket_addr));
        assert_eq!(mapper.lookup(2000), None);
    }

    #[test]
    fn test_address_mapper_remove() {
        let mapper = AddressMapper::new();

        let socket_addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        mapper.add_mapping(1000, socket_addr);

        assert_eq!(mapper.mapping_count(), 1);

        mapper.remove_mapping(1000);
        assert_eq!(mapper.mapping_count(), 0);
        assert_eq!(mapper.lookup(1000), None);
    }

    #[test]
    fn test_address_mapper_multiple() {
        let mapper = AddressMapper::new();

        let addr1: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let addr2: SocketAddr = "127.0.0.1:8081".parse().unwrap();

        mapper.add_mapping(1000, addr1);
        mapper.add_mapping(2000, addr2);

        assert_eq!(mapper.mapping_count(), 2);
        assert_eq!(mapper.lookup(1000), Some(addr1));
        assert_eq!(mapper.lookup(2000), Some(addr2));
    }
}
