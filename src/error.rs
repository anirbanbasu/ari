// SPDX-License-Identifier: EUPL-1.2-or-later
// Copyright © 2026-present ARI Contributors

//! Error types for ARI
//!
//! This module provides typed errors for all RINA components,
//! replacing string-based errors with structured error types.

use thiserror::Error;

/// Main error type for ARI operations
#[derive(Error, Debug)]
pub enum AriError {
    #[error("Enrollment error: {0}")]
    Enrollment(#[from] EnrollmentError),

    #[error("RIB error: {0}")]
    Rib(#[from] RibError),

    #[error("RMT error: {0}")]
    Rmt(#[from] RmtError),

    #[error("EFCP error: {0}")]
    Efcp(#[from] EfcpError),

    #[error("Shim error: {0}")]
    Shim(#[from] ShimError),

    #[error("CDAP error: {0}")]
    Cdap(#[from] CdapError),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] SerializationError),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Actor channel closed")]
    ChannelClosed,

    #[error("Operation timed out")]
    Timeout,

    #[error("Feature not implemented: {0}")]
    NotImplemented(String),
}

/// Enrollment-specific errors
#[derive(Error, Debug, Clone)]
pub enum EnrollmentError {
    #[error("Not enrolled in any DIF")]
    NotEnrolled,

    #[error("Already enrolled in DIF: {0}")]
    AlreadyEnrolled(String),

    #[error("Enrollment request rejected: {0}")]
    Rejected(String),

    #[error("Enrollment timeout after {attempts} attempts")]
    Timeout { attempts: u32 },

    #[error("Invalid enrollment state: expected {expected}, got {actual}")]
    InvalidState { expected: String, actual: String },

    #[error("Bootstrap peer not reachable: {0}")]
    PeerUnreachable(String),

    #[error("No bootstrap peers configured")]
    NoBootstrapPeers,

    #[error("IPCP name not set")]
    IpcpNameNotSet,

    #[error("Failed to serialize enrollment request: {0}")]
    SerializationFailed(String),

    #[error("Failed to deserialize enrollment response: {0}")]
    DeserializationFailed(String),

    #[error("Failed to send enrollment request: {0}")]
    SendFailed(String),

    #[error("Failed to receive enrollment response: {0}")]
    ReceiveFailed(String),

    #[error("Invalid response format: {0}")]
    InvalidResponse(String),

    #[error("Address assignment failed: {0}")]
    AddressAssignmentFailed(String),

    #[error("RIB synchronization failed: {0}")]
    RibSyncFailed(String),

    #[error("Connection lost to bootstrap")]
    ConnectionLost,

    #[error("Re-enrollment required")]
    ReEnrollmentRequired,
}

/// RIB-specific errors
#[derive(Error, Debug, Clone)]
pub enum RibError {
    #[error("Object not found: {0}")]
    NotFound(String),

    #[error("Object already exists: {0}")]
    AlreadyExists(String),

    #[error("Invalid object name: {0}")]
    InvalidName(String),

    #[error("Invalid object class: {0}")]
    InvalidClass(String),

    #[error("Serialization failed: {0}")]
    SerializationFailed(String),

    #[error("Deserialization failed: {0}")]
    DeserializationFailed(String),

    #[error("Access denied: {0}")]
    AccessDenied(String),

    #[error("RIB operation failed: {0}")]
    OperationFailed(String),
}

/// RMT-specific errors
#[derive(Error, Debug, Clone)]
pub enum RmtError {
    #[error("No route to destination: {0}")]
    NoRoute(u64),

    #[error("Route not found for destination: {0}")]
    RouteNotFound(u64),

    #[error("Queue full for next hop: {0}")]
    QueueFull(u64),

    #[error("Invalid PDU: {0}")]
    InvalidPdu(String),

    #[error("Forwarding failed: {0}")]
    ForwardingFailed(String),

    #[error("Next hop unreachable: {0}")]
    NextHopUnreachable(u64),

    #[error("Network error: {0}")]
    Network(String),
}

/// EFCP-specific errors
#[derive(Error, Debug, Clone)]
pub enum EfcpError {
    #[error("Flow not found: {0}")]
    FlowNotFound(u64),

    #[error("Flow already exists: {0}")]
    FlowAlreadyExists(u64),

    #[error("Flow allocation failed: {0}")]
    AllocationFailed(String),

    #[error("Invalid flow configuration: {0}")]
    InvalidConfig(String),

    #[error("Send failed: {0}")]
    SendFailed(String),

    #[error("Receive failed: {0}")]
    ReceiveFailed(String),

    #[error("Flow closed: {0}")]
    FlowClosed(u64),

    #[error("Sequence number error: expected {expected}, got {actual}")]
    SequenceError { expected: u64, actual: u64 },
}

/// Shim layer errors
#[derive(Error, Debug, Clone)]
pub enum ShimError {
    #[error("Failed to bind socket: {0}")]
    BindFailed(String),

    #[error("Send failed: {0}")]
    SendFailed(String),

    #[error("Receive failed: {0}")]
    ReceiveFailed(String),

    #[error("Invalid socket address: {0}")]
    InvalidAddress(String),

    #[error("Peer not registered: {0}")]
    PeerNotRegistered(u64),

    #[error("Socket closed")]
    SocketClosed,

    #[error("I/O error: {0}")]
    IoError(String),
}

/// CDAP-specific errors
#[derive(Error, Debug, Clone)]
pub enum CdapError {
    #[error("Invalid operation code: {0}")]
    InvalidOpCode(u8),

    #[error("Invalid message format: {0}")]
    InvalidFormat(String),

    #[error("Operation failed: {0}")]
    OperationFailed(String),

    #[error("Invoke ID mismatch: expected {expected}, got {actual}")]
    InvokeIdMismatch { expected: u32, actual: u32 },

    #[error("Object not found: {0}")]
    ObjectNotFound(String),

    #[error("Session error: {0}")]
    SessionError(String),
}

/// Serialization/deserialization errors
#[derive(Error, Debug)]
pub enum SerializationError {
    #[error("Postcard serialization failed: {0}")]
    PostcardSerialization(#[from] postcard::Error),

    #[error("JSON serialization failed: {0}")]
    JsonSerialization(#[from] serde_json::Error),

    #[error("Invalid data format: {0}")]
    InvalidFormat(String),
}

// Conversion from String for backwards compatibility during migration
impl From<String> for AriError {
    fn from(s: String) -> Self {
        AriError::Config(s)
    }
}

impl From<&str> for AriError {
    fn from(s: &str) -> Self {
        AriError::Config(s.to_string())
    }
}

// Enable conversion to String for backwards compatibility
impl From<AriError> for String {
    fn from(err: AriError) -> Self {
        err.to_string()
    }
}

impl From<EnrollmentError> for String {
    fn from(err: EnrollmentError) -> Self {
        err.to_string()
    }
}

impl From<RibError> for String {
    fn from(err: RibError) -> Self {
        err.to_string()
    }
}

impl From<RmtError> for String {
    fn from(err: RmtError) -> Self {
        err.to_string()
    }
}

impl From<EfcpError> for String {
    fn from(err: EfcpError) -> Self {
        err.to_string()
    }
}

impl From<ShimError> for String {
    fn from(err: ShimError) -> Self {
        err.to_string()
    }
}

impl From<CdapError> for String {
    fn from(err: CdapError) -> Self {
        err.to_string()
    }
}
