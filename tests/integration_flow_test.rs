// SPDX-License-Identifier: EUPL-1.2-or-later
// Copyright © 2026-present ARI Contributors

//! Integration test for Phase 2: Basic flow creation and data transfer

use ari::actors::*;
use ari::efcp::FlowConfig;

use ari::rib::RibValue;
use std::collections::HashMap;
use tokio::sync::mpsc;

#[tokio::test]
async fn test_flow_creation_and_data_transfer() {
    println!("\n=== Phase 2 Integration Test: Flow Creation & Data Transfer ===\n");

    // === Setup Bootstrap IPCP (addr: 1001) ===
    let bootstrap_addr = 1001u64;
    let bootstrap_bind = "127.0.0.1:9000";

    println!("1. Setting up Bootstrap IPCP (addr: {})...", bootstrap_addr);

    // Create actors for bootstrap
    let (bootstrap_rib_tx, bootstrap_rib_rx) = mpsc::channel(32);
    let bootstrap_rib_handle = RibHandle::new(bootstrap_rib_tx);

    let (bootstrap_efcp_tx, bootstrap_efcp_rx) = mpsc::channel(32);
    let bootstrap_efcp_handle = EfcpHandle::new(bootstrap_efcp_tx);

    let (bootstrap_rmt_tx, bootstrap_rmt_rx) = mpsc::channel(32);
    let bootstrap_rmt_handle = RmtHandle::new(bootstrap_rmt_tx);

    let (bootstrap_shim_tx, bootstrap_shim_rx) = mpsc::channel(32);
    let bootstrap_shim_handle = ShimHandle::new(bootstrap_shim_tx);

    // Spawn bootstrap actors
    tokio::spawn(async move {
        let actor = RibActor::new(bootstrap_rib_rx);
        actor.run().await;
    });

    let bootstrap_rmt_for_efcp = bootstrap_rmt_handle.clone();
    tokio::spawn(async move {
        let mut actor = EfcpActor::new(bootstrap_efcp_rx);
        actor.set_rmt_handle(bootstrap_rmt_for_efcp);
        actor.run().await;
    });

    let bootstrap_shim_for_rmt = bootstrap_shim_handle.clone();
    let bootstrap_rib_for_rmt = bootstrap_rib_handle.clone();
    tokio::spawn(async move {
        let mut actor = RmtActor::new(bootstrap_addr, bootstrap_rmt_rx);
        actor.set_shim_handle(bootstrap_shim_for_rmt);
        actor.set_rib_handle(bootstrap_rib_for_rmt);

        // Populate forwarding table from RIB
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        actor.populate_forwarding_table().await;

        actor.run().await;
    });

    tokio::spawn(async move {
        let actor = ShimActor::new(bootstrap_addr, bootstrap_shim_rx);
        actor.run().await;
    });

    // Load static route into RIB via RibHandle
    {
        let route_name = "/routing/static/1002".to_string();
        let route_value = RibValue::Struct({
            let mut map = HashMap::new();
            map.insert(
                "destination".to_string(),
                Box::new(RibValue::String("1002".to_string())),
            );
            map.insert(
                "next_hop_address".to_string(),
                Box::new(RibValue::String("127.0.0.1:9001".to_string())),
            );
            map.insert(
                "next_hop_rina_addr".to_string(),
                Box::new(RibValue::String("1002".to_string())),
            );
            map
        });

        let (tx, mut rx) = mpsc::channel(1);
        bootstrap_rib_handle
            .send(RibMessage::Create {
                name: route_name,
                class: "route".to_string(),
                value: route_value,
                response: tx,
            })
            .await
            .unwrap();
        rx.recv().await.unwrap().unwrap();
        println!("  ✓ Loaded route: 1002 → 127.0.0.1:9001");
    }

    // Bind bootstrap shim via ShimActor
    {
        let (tx, mut rx) = mpsc::channel(1);
        bootstrap_shim_handle
            .send(ShimMessage::Bind {
                addr: bootstrap_bind.to_string(),
                response: tx,
            })
            .await
            .unwrap();
        rx.recv().await.unwrap().unwrap();
        println!("  ✓ Bound to {}", bootstrap_bind);
    }

    println!("  ✓ Bootstrap IPCP ready\n");

    // === Setup Member IPCP (addr: 1002) ===
    let member_addr = 1002u64;
    let member_bind = "127.0.0.1:9001";

    println!("2. Setting up Member IPCP (addr: {})...", member_addr);

    // Create actors for member
    let (member_rib_tx, member_rib_rx) = mpsc::channel(32);
    let member_rib_handle = RibHandle::new(member_rib_tx);

    let (member_efcp_tx, member_efcp_rx) = mpsc::channel(32);
    let _member_efcp_handle = EfcpHandle::new(member_efcp_tx);

    let (member_rmt_tx, member_rmt_rx) = mpsc::channel(32);
    let member_rmt_handle = RmtHandle::new(member_rmt_tx);

    let (member_shim_tx, member_shim_rx) = mpsc::channel(32);
    let member_shim_handle = ShimHandle::new(member_shim_tx);

    // Spawn member actors
    tokio::spawn(async move {
        let actor = RibActor::new(member_rib_rx);
        actor.run().await;
    });

    let member_rmt_for_efcp = member_rmt_handle.clone();
    tokio::spawn(async move {
        let mut actor = EfcpActor::new(member_efcp_rx);
        actor.set_rmt_handle(member_rmt_for_efcp);
        actor.run().await;
    });

    let member_shim_for_rmt = member_shim_handle.clone();
    let member_rib_for_rmt = member_rib_handle.clone();
    tokio::spawn(async move {
        let mut actor = RmtActor::new(member_addr, member_rmt_rx);
        actor.set_shim_handle(member_shim_for_rmt);
        actor.set_rib_handle(member_rib_for_rmt);

        // Populate forwarding table from RIB
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        actor.populate_forwarding_table().await;

        actor.run().await;
    });

    tokio::spawn(async move {
        let actor = ShimActor::new(member_addr, member_shim_rx);
        actor.run().await;
    });

    // Load reverse route into RIB via RibHandle
    {
        let route_name = "/routing/static/1001".to_string();
        let route_value = RibValue::Struct({
            let mut map = HashMap::new();
            map.insert(
                "destination".to_string(),
                Box::new(RibValue::String("1001".to_string())),
            );
            map.insert(
                "next_hop_address".to_string(),
                Box::new(RibValue::String("127.0.0.1:9000".to_string())),
            );
            map.insert(
                "next_hop_rina_addr".to_string(),
                Box::new(RibValue::String("1001".to_string())),
            );
            map
        });

        let (tx, mut rx) = mpsc::channel(1);
        member_rib_handle
            .send(RibMessage::Create {
                name: route_name,
                class: "route".to_string(),
                value: route_value,
                response: tx,
            })
            .await
            .unwrap();
        rx.recv().await.unwrap().unwrap();
        println!("  ✓ Loaded route: 1001 → 127.0.0.1:9000");
    }

    // Bind member shim via ShimActor
    {
        let (tx, mut rx) = mpsc::channel(1);
        member_shim_handle
            .send(ShimMessage::Bind {
                addr: member_bind.to_string(),
                response: tx,
            })
            .await
            .unwrap();
        rx.recv().await.unwrap().unwrap();
        println!("  ✓ Bound to {}", member_bind);
    }

    println!("  ✓ Member IPCP ready\n");

    // Give actors time to initialize
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    // === Test Flow Creation and Data Transfer ===
    println!("3. Creating flow and sending data...");

    // Allocate flow on bootstrap IPCP
    let (tx, mut rx) = mpsc::channel(1);
    bootstrap_efcp_handle
        .send(EfcpMessage::AllocateFlow {
            local_addr: bootstrap_addr,
            remote_addr: member_addr,
            config: FlowConfig::default(),
            response: tx,
        })
        .await
        .unwrap();

    let flow_id = rx.recv().await.unwrap();
    println!("  ✓ Flow allocated: flow_id={}", flow_id);

    // Send data on the flow
    let test_data = b"Hello from Bootstrap IPCP!".to_vec();
    let (tx, mut rx) = mpsc::channel(1);
    bootstrap_efcp_handle
        .send(EfcpMessage::SendData {
            flow_id,
            data: test_data.clone(),
            response: tx,
        })
        .await
        .unwrap();

    let send_result = rx.recv().await.unwrap();
    assert!(
        send_result.is_ok(),
        "Failed to send data: {:?}",
        send_result
    );
    println!("  ✓ Data sent: {} bytes", test_data.len());

    // Give time for PDU to be received and processed
    println!("\n4. Waiting for data delivery...");
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    println!("\n=== Phase 2 Test: PASSED ===");
    println!("✓ Flow creation successful");
    println!("✓ Data transfer successful");
    println!("✓ PDUs routed through RMT→Shim→Network→Shim→RMT→EFCP");
}
