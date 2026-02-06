// SPDX-License-Identifier: EUPL-1.2-or-later
// Copyright © 2026-present ARI Contributors

//! Actor-based components using Tokio
//!
//! This module provides async actors for each RINA component,
//! allowing them to run concurrently and communicate via channels.

use crate::efcp::{Efcp, FlowConfig};
use crate::inter_ipcp_fal::InterIpcpFlowAllocator;
use crate::pdu::Pdu;
use crate::rib::{Rib, RibValue};
use crate::rmt::{ForwardingEntry, Rmt};
use crate::routing::RouteResolver;
use crate::shim::UdpShim;
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc};

/// Messages for RIB actor
#[derive(Debug)]
pub enum RibMessage {
    Create {
        name: String,
        class: String,
        value: RibValue,
        response: mpsc::Sender<Result<(), String>>,
    },
    Read {
        name: String,
        response: mpsc::Sender<Option<RibValue>>,
    },
    Update {
        name: String,
        value: RibValue,
        response: mpsc::Sender<Result<(), String>>,
    },
    Delete {
        name: String,
        response: mpsc::Sender<Result<(), String>>,
    },
    ListByClass {
        class: String,
        response: mpsc::Sender<Vec<String>>,
    },
    Count {
        response: mpsc::Sender<usize>,
    },
}

/// RIB Actor - manages Resource Information Base
pub struct RibActor {
    rib: Arc<RwLock<Rib>>,
    receiver: mpsc::Receiver<RibMessage>,
}

impl RibActor {
    pub fn new(receiver: mpsc::Receiver<RibMessage>) -> Self {
        Self {
            rib: Arc::new(RwLock::new(Rib::new())),
            receiver,
        }
    }

    pub async fn run(mut self) {
        while let Some(msg) = self.receiver.recv().await {
            match msg {
                RibMessage::Create {
                    name,
                    class,
                    value,
                    response,
                } => {
                    let rib = self.rib.read().await;
                    let result = rib.create(name, class, value).await;
                    let _ = response.send(result).await;
                }
                RibMessage::Read { name, response } => {
                    let rib = self.rib.read().await;
                    let obj = rib.read(&name).await;
                    let _ = response.send(obj.map(|o| o.value)).await;
                }
                RibMessage::Update {
                    name,
                    value,
                    response,
                } => {
                    let rib = self.rib.read().await;
                    let result = rib.update(&name, value).await;
                    let _ = response.send(result).await;
                }
                RibMessage::Delete { name, response } => {
                    let rib = self.rib.read().await;
                    let result = rib.delete(&name).await;
                    let _ = response.send(result).await;
                }
                RibMessage::ListByClass { class, response } => {
                    let rib = self.rib.read().await;
                    let list = rib.list_by_class(&class).await;
                    let _ = response.send(list).await;
                }
                RibMessage::Count { response } => {
                    let rib = self.rib.read().await;
                    let count = rib.count().await;
                    let _ = response.send(count).await;
                }
            }
        }
    }
}

/// Messages for EFCP actor
#[derive(Debug)]
pub enum EfcpMessage {
    AllocateFlow {
        local_addr: u64,
        remote_addr: u64,
        config: FlowConfig,
        response: mpsc::Sender<u32>,
    },
    SendData {
        flow_id: u32,
        data: Vec<u8>,
        response: mpsc::Sender<Result<Pdu, String>>,
    },
    ReceivePdu {
        pdu: Pdu,
        response: mpsc::Sender<Result<Option<Vec<u8>>, String>>,
    },
    DeallocateFlow {
        flow_id: u32,
        response: mpsc::Sender<Result<(), String>>,
    },
    GetFlowCount {
        response: mpsc::Sender<usize>,
    },
}

/// EFCP Actor - manages flows and data transfer
pub struct EfcpActor {
    efcp: Arc<RwLock<Efcp>>,
    receiver: mpsc::Receiver<EfcpMessage>,
    rmt_handle: Option<RmtHandle>,
}

impl EfcpActor {
    pub fn new(receiver: mpsc::Receiver<EfcpMessage>) -> Self {
        Self {
            efcp: Arc::new(RwLock::new(Efcp::new())),
            receiver,
            rmt_handle: None,
        }
    }

    pub fn set_rmt_handle(&mut self, handle: RmtHandle) {
        self.rmt_handle = Some(handle);
    }

    pub async fn run(mut self) {
        while let Some(msg) = self.receiver.recv().await {
            match msg {
                EfcpMessage::AllocateFlow {
                    local_addr,
                    remote_addr,
                    config,
                    response,
                } => {
                    let mut efcp = self.efcp.write().await;
                    let flow_id = efcp.allocate_flow(local_addr, remote_addr, config);
                    let _ = response.send(flow_id).await;
                }
                EfcpMessage::SendData {
                    flow_id,
                    data,
                    response,
                } => {
                    let mut efcp = self.efcp.write().await;
                    let result = efcp
                        .get_flow_mut(flow_id)
                        .ok_or_else(|| format!("Flow {} not found", flow_id))
                        .and_then(|flow| flow.send_data(data));

                    // Forward PDU to RMT if successful
                    if let (Ok(pdu), Some(rmt_handle)) = (&result, &self.rmt_handle) {
                        let (tx, mut rx) = mpsc::channel(1);
                        if (rmt_handle
                            .sender
                            .send(RmtMessage::ProcessOutgoing {
                                pdu: pdu.clone(),
                                response: tx,
                            })
                            .await)
                            .is_ok()
                        {
                            let _ = rx.recv().await;
                        }
                    }

                    let _ = response.send(result).await;
                }
                EfcpMessage::ReceivePdu { pdu, response } => {
                    let mut efcp = self.efcp.write().await;
                    let flow_id = pdu.dst_cep_id;
                    let result = efcp
                        .get_flow_mut(flow_id)
                        .ok_or_else(|| format!("Flow {} not found", flow_id))
                        .and_then(|flow| flow.receive_pdu(pdu));
                    let _ = response.send(result).await;
                }
                EfcpMessage::DeallocateFlow { flow_id, response } => {
                    let mut efcp = self.efcp.write().await;
                    let result = efcp.deallocate_flow(flow_id);
                    let _ = response.send(result).await;
                }
                EfcpMessage::GetFlowCount { response } => {
                    let efcp = self.efcp.read().await;
                    let count = efcp.flow_count();
                    let _ = response.send(count).await;
                }
            }
        }
    }
}

/// Messages for RMT actor
#[derive(Debug)]
pub enum RmtMessage {
    AddForwardingEntry {
        entry: ForwardingEntry,
        response: mpsc::Sender<()>,
    },
    ProcessOutgoing {
        pdu: Pdu,
        response: mpsc::Sender<Result<u64, String>>,
    },
    ProcessIncoming {
        pdu: Pdu,
        response: mpsc::Sender<Result<Option<u64>, String>>,
    },
    DequeueForNextHop {
        next_hop: u64,
        response: mpsc::Sender<Option<Pdu>>,
    },
    GetForwardingTableSize {
        response: mpsc::Sender<usize>,
    },
}

/// RMT Actor - handles relaying and multiplexing
pub struct RmtActor {
    rmt: Arc<RwLock<Rmt>>,
    receiver: mpsc::Receiver<RmtMessage>,
    flow_allocator: Option<Arc<InterIpcpFlowAllocator>>,
    route_resolver: Option<Arc<RouteResolver>>,
}

impl RmtActor {
    pub fn new(local_addr: u64, receiver: mpsc::Receiver<RmtMessage>) -> Self {
        Self {
            rmt: Arc::new(RwLock::new(Rmt::new(local_addr))),
            receiver,
            flow_allocator: None,
            route_resolver: None,
        }
    }

    pub fn set_flow_allocator(&mut self, allocator: Arc<InterIpcpFlowAllocator>) {
        self.flow_allocator = Some(allocator);
    }

    pub fn set_route_resolver(&mut self, resolver: Arc<RouteResolver>) {
        self.route_resolver = Some(resolver);
    }

    /// Populate forwarding table from RIB routes
    ///
    /// DEPRECATED: With RouteResolver, forwarding is done via next-hop resolution
    /// rather than pre-populating a forwarding table. This method is kept for
    /// backward compatibility but may be removed in future versions.
    pub async fn populate_forwarding_table(&self) {
        // No-op: RouteResolver handles route lookups dynamically
        println!("⚠️  populate_forwarding_table() is deprecated - using RouteResolver instead");
    }

    pub async fn run(mut self) {
        while let Some(msg) = self.receiver.recv().await {
            match msg {
                RmtMessage::AddForwardingEntry { entry, response } => {
                    let mut rmt = self.rmt.write().await;
                    rmt.add_forwarding_entry(entry);
                    let _ = response.send(()).await;
                }
                RmtMessage::ProcessOutgoing { pdu, response } => {
                    let mut rmt = self.rmt.write().await;
                    let result = rmt.process_outgoing(pdu.clone());

                    if result.is_ok() {
                        // Use flow allocator to send PDU
                        if let Some(flow_allocator) = &self.flow_allocator {
                            match flow_allocator.send_pdu(pdu.dst_addr, &pdu) {
                                Ok(_) => {
                                    println!(
                                        "📤 Sent PDU to {} via InterIpcpFlowAllocator",
                                        pdu.dst_addr
                                    );
                                }
                                Err(e) => {
                                    eprintln!("❌ Failed to send PDU via flow allocator: {}", e);
                                    let _ = response
                                        .send(Err(format!("Flow allocator error: {}", e)))
                                        .await;
                                    continue;
                                }
                            }
                        } else {
                            eprintln!("❌ InterIpcpFlowAllocator not initialized for RMT");
                            let _ = response
                                .send(Err("Flow allocator not initialized".to_string()))
                                .await;
                            continue;
                        }
                    }

                    let _ = response.send(result).await;
                }
                RmtMessage::ProcessIncoming { pdu, response } => {
                    let mut rmt = self.rmt.write().await;
                    let result = rmt.process_incoming(pdu);
                    let _ = response.send(result).await;
                }
                RmtMessage::DequeueForNextHop { next_hop, response } => {
                    let mut rmt = self.rmt.write().await;
                    let pdu = rmt.dequeue_for_next_hop(next_hop);
                    let _ = response.send(pdu).await;
                }
                RmtMessage::GetForwardingTableSize { response } => {
                    let rmt = self.rmt.read().await;
                    let size = rmt.forwarding_table_size();
                    let _ = response.send(size).await;
                }
            }
        }
    }
}

/// Messages for Shim actor
#[derive(Debug)]
pub enum ShimMessage {
    Bind {
        addr: String,
        response: mpsc::Sender<Result<(), String>>,
    },
    Send {
        data: Vec<u8>,
        dest: String,
        response: mpsc::Sender<Result<usize, String>>,
    },
    GetLocalAddr {
        response: mpsc::Sender<Result<String, String>>,
    },
}

/// Shim Actor - handles UDP/IP networking
pub struct ShimActor {
    shim: Arc<RwLock<UdpShim>>,
    receiver: mpsc::Receiver<ShimMessage>,
}

impl ShimActor {
    pub fn new(local_rina_addr: u64, receiver: mpsc::Receiver<ShimMessage>) -> Self {
        Self {
            shim: Arc::new(RwLock::new(UdpShim::new(local_rina_addr))),
            receiver,
        }
    }

    pub async fn run(mut self) {
        while let Some(msg) = self.receiver.recv().await {
            match msg {
                ShimMessage::Bind { addr, response } => {
                    let shim = self.shim.read().await;
                    let result = shim.bind(&addr).map_err(|e| e.to_string());
                    let _ = response.send(result).await;
                }
                ShimMessage::Send {
                    data,
                    dest,
                    response,
                } => {
                    let shim = self.shim.read().await;
                    let result = shim.send_to(&data, &dest).map_err(|e| e.to_string());
                    let _ = response.send(result).await;
                }
                ShimMessage::GetLocalAddr { response } => {
                    let shim = self.shim.read().await;
                    let result = shim
                        .local_addr()
                        .map(|a| a.to_string())
                        .map_err(|e| e.to_string());
                    let _ = response.send(result).await;
                }
            }
        }
    }

    /// Spawns a receiver task that continuously receives packets and processes them through RMT
    pub async fn spawn_receiver(
        shim: Arc<RwLock<UdpShim>>,
        rmt_handle: RmtHandle,
        efcp_handle: EfcpHandle,
        local_rina_addr: u64,
        mut receiver_shutdown: mpsc::Receiver<()>,
    ) {
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = receiver_shutdown.recv() => {
                        break;
                    }
                    _ = tokio::time::sleep(tokio::time::Duration::from_millis(10)) => {
                        let shim = shim.read().await;
                        if let Ok(Some((pdu_bytes, src))) = shim.recv_from() {
                            // Deserialize PDU
                            match bincode::deserialize::<Pdu>(&pdu_bytes) {
                                Ok(pdu) => {
                                    println!("📥 Received PDU from {} → dst:{} ({}bytes)",
                                        src, pdu.dst_addr, pdu_bytes.len());

                                    // Send to RMT for processing
                                    let (resp_tx, mut resp_rx) = mpsc::channel(1);
                                    let _ = rmt_handle.send(RmtMessage::ProcessIncoming {
                                        pdu: pdu.clone(),
                                        response: resp_tx,
                                    }).await;

                                    // Check if PDU is for local delivery
                                    if let Some(Ok(Some(local_addr))) = resp_rx.recv().await {
                                        if local_addr == local_rina_addr {
                                            println!("  ✓ PDU is for local delivery, passing to EFCP");

                                            // Deliver to EFCP
                                            let (efcp_tx, mut efcp_rx) = mpsc::channel(1);
                                            let _ = efcp_handle.send(EfcpMessage::ReceivePdu {
                                                pdu,
                                                response: efcp_tx,
                                            }).await;

                                            if let Some(Ok(Some(data))) = efcp_rx.recv().await {
                                                println!("  ✓ EFCP delivered {} bytes of data", data.len());
                                            }
                                        } else {
                                            println!("  → PDU queued for forwarding to {}", local_addr);
                                        }
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Failed to deserialize PDU: {}", e);
                                }
                            }
                        }
                    }
                }
            }
        });
    }
}

/// Actor handle for sending messages to an actor
pub struct ActorHandle<T> {
    sender: mpsc::Sender<T>,
}

impl<T> Clone for ActorHandle<T> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
        }
    }
}

impl<T> ActorHandle<T> {
    pub fn new(sender: mpsc::Sender<T>) -> Self {
        Self { sender }
    }

    pub async fn send(&self, msg: T) -> Result<(), String> {
        self.sender
            .send(msg)
            .await
            .map_err(|_| "Failed to send message".to_string())
    }
}

pub type RibHandle = ActorHandle<RibMessage>;
pub type EfcpHandle = ActorHandle<EfcpMessage>;
pub type RmtHandle = ActorHandle<RmtMessage>;
pub type ShimHandle = ActorHandle<ShimMessage>;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rib_actor_create_and_read() {
        let (tx, rx) = mpsc::channel(32);
        let actor = RibActor::new(rx);

        tokio::spawn(async move {
            actor.run().await;
        });

        let handle = RibHandle::new(tx);

        // Create
        let (resp_tx, mut resp_rx) = mpsc::channel(1);
        handle
            .send(RibMessage::Create {
                name: "test".to_string(),
                class: "test".to_string(),
                value: RibValue::Integer(42),
                response: resp_tx,
            })
            .await
            .unwrap();

        let result = resp_rx.recv().await.unwrap();
        assert!(result.is_ok());

        // Read
        let (resp_tx, mut resp_rx) = mpsc::channel(1);
        handle
            .send(RibMessage::Read {
                name: "test".to_string(),
                response: resp_tx,
            })
            .await
            .unwrap();

        let value = resp_rx.recv().await.unwrap();
        assert!(value.is_some());
        assert_eq!(value.unwrap().as_integer(), Some(42));
    }

    #[tokio::test]
    async fn test_efcp_actor_allocate_flow() {
        let (tx, rx) = mpsc::channel(32);
        let actor = EfcpActor::new(rx);

        tokio::spawn(async move {
            actor.run().await;
        });

        let handle = EfcpHandle::new(tx);

        let (resp_tx, mut resp_rx) = mpsc::channel(1);
        handle
            .send(EfcpMessage::AllocateFlow {
                local_addr: 1000,
                remote_addr: 2000,
                config: FlowConfig::default(),
                response: resp_tx,
            })
            .await
            .unwrap();

        let flow_id = resp_rx.recv().await.unwrap();
        assert_eq!(flow_id, 1);
    }
}
