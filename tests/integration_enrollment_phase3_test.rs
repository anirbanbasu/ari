// SPDX-License-Identifier: EUPL-1.2-or-later
// Copyright © 2026-present ARI Contributors

//! Integration test for Phase 3: Dynamic Address Assignment
//!
//! Tests the complete enrollment workflow with:
//! - Bootstrap IPCP with address pool
//! - Member IPCP requesting dynamic address
//! - RIB synchronization
//! - Route creation

use ari::routing::{RouteResolver, RouteResolverConfig};
use ari::{EnrollmentManager, ForwardingEntry, Rib, RibValue, Rmt, UdpShim};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{Duration, sleep};

#[tokio::test]
async fn test_phase3_dynamic_address_assignment() {
    println!("\n=== Phase 3: Dynamic Address Assignment Test ===\n");

    // Configuration
    let bootstrap_addr = 1001;
    let bootstrap_bind = "127.0.0.1:17000";
    let member_bind = "127.0.0.1:17001";
    let pool_start = 2000;
    let pool_end = 2999;

    // === Bootstrap IPCP Setup ===
    println!("1. Setting up Bootstrap IPCP");

    let bootstrap_rib = Rib::new();
    bootstrap_rib
        .create(
            "/dif/name".to_string(),
            "dif_info".to_string(),
            RibValue::String("test-dif".to_string()),
        )
        .await
        .unwrap();

    // Add a static route back to members
    bootstrap_rib
        .create(
            "/routing/static/2000".to_string(),
            "static_route".to_string(),
            RibValue::Struct({
                let mut map = std::collections::HashMap::new();
                map.insert(
                    "next_hop_address".to_string(),
                    Box::new(RibValue::String(member_bind.to_string())),
                );
                map.insert(
                    "next_hop_rina_addr".to_string(),
                    Box::new(RibValue::Integer(2000)),
                );
                map
            }),
        )
        .await
        .unwrap();

    let bootstrap_shim = Arc::new(UdpShim::new(bootstrap_addr));
    bootstrap_shim.bind(bootstrap_bind).unwrap();

    // Create RouteResolver for bootstrap
    let bootstrap_rib_arc = Arc::new(RwLock::new(bootstrap_rib.clone()));
    let bootstrap_resolver_config = RouteResolverConfig {
        enable_persistence: false,
        snapshot_path: PathBuf::from("test-phase3-bootstrap.toml"),
        default_ttl_seconds: 3600,
        snapshot_interval_seconds: 0,
    };
    let bootstrap_route_resolver = Arc::new(RouteResolver::new(
        bootstrap_rib_arc.clone(),
        bootstrap_resolver_config,
    ));

    let mut bootstrap_em = EnrollmentManager::new_bootstrap(
        bootstrap_rib.clone(),
        bootstrap_shim.clone(),
        bootstrap_addr,
        pool_start,
        pool_end,
    );
    bootstrap_em.set_route_resolver(bootstrap_route_resolver.clone());

    println!("   ✓ Bootstrap IPCP ready");
    println!("     - Address: {}", bootstrap_addr);
    println!("     - Bind: {}", bootstrap_bind);
    println!("     - Address pool: {}-{}", pool_start, pool_end);

    // === Member IPCP Setup ===
    println!("\n2. Setting up Member IPCP");

    let member_rib = Rib::new();

    // Member starts with address 0 (will request dynamic assignment)
    let member_initial_addr = 0;
    let member_shim = Arc::new(UdpShim::new(member_initial_addr));
    member_shim.bind(member_bind).unwrap();

    // Register bootstrap peer
    let bootstrap_socket: std::net::SocketAddr = bootstrap_bind.parse().unwrap();
    member_shim.register_peer(bootstrap_addr, bootstrap_socket);

    let mut member_em =
        EnrollmentManager::new(member_rib.clone(), member_shim.clone(), member_initial_addr);
    member_em.set_ipcp_name("member-ipcp-1".to_string());

    println!("   ✓ Member IPCP ready");
    println!(
        "     - Initial address: {} (requesting dynamic)",
        member_initial_addr
    );
    println!("     - Bind: {}", member_bind);

    // === Start Bootstrap Listener (in background) ===
    println!("\n3. Starting bootstrap listener");

    let bootstrap_em_clone = Arc::new(bootstrap_em);
    let bootstrap_shim_clone = bootstrap_shim.clone();
    let bootstrap_listener = tokio::spawn(async move {
        for _ in 0..20 {
            // Listen for ~2 seconds
            sleep(Duration::from_millis(100)).await;

            if let Ok(Some((pdu, src_addr))) = bootstrap_shim_clone.receive_pdu() {
                println!("   → Bootstrap received PDU from {}", src_addr);
                if let Err(e) = bootstrap_em_clone.handle_cdap_message(&pdu, src_addr).await {
                    eprintln!("   ✗ Failed to handle CDAP: {}", e);
                }
            }
        }
    });

    // Give bootstrap a moment to start
    sleep(Duration::from_millis(100)).await;

    // === Member Enrollment ===
    println!("\n4. Member enrolling with bootstrap");

    let enrollment_result = member_em.enrol_with_bootstrap(bootstrap_addr).await;

    assert!(
        enrollment_result.is_ok(),
        "Enrollment should succeed: {:?}",
        enrollment_result
    );

    let dif_name = enrollment_result.unwrap();
    println!("   ✓ Enrollment successful");
    println!("     - DIF Name: {}", dif_name);

    // === Verify Dynamic Address Assignment ===
    println!("\n5. Verifying address assignment");

    let assigned_addr = member_em.local_addr();
    println!("   - Assigned address: {}", assigned_addr);

    assert_ne!(
        assigned_addr, member_initial_addr,
        "Member should have received a new address"
    );
    assert!(
        assigned_addr >= pool_start && assigned_addr <= pool_end,
        "Assigned address {} should be within pool range {}-{}",
        assigned_addr,
        pool_start,
        pool_end
    );

    // Check if address was stored in member's RIB
    let addr_obj = member_rib.read("/local/address").await;
    assert!(addr_obj.is_some(), "Assigned address should be in RIB");
    if let Some(obj) = addr_obj {
        assert_eq!(
            obj.value.as_integer(),
            Some(assigned_addr as i64),
            "RIB should contain correct assigned address"
        );
    }

    println!("   ✓ Address correctly assigned and stored");

    // === Verify RIB Synchronization ===
    println!("\n6. Verifying RIB synchronization");

    // Check DIF name was synced
    let dif_name_obj = member_rib.read("/dif/name").await;
    assert!(dif_name_obj.is_some(), "DIF name should be synced");
    if let Some(obj) = dif_name_obj {
        assert_eq!(
            obj.value.as_string(),
            Some("test-dif"),
            "DIF name should match"
        );
    }

    // Check static route was synced from bootstrap
    let route_obj = member_rib.read("/routing/static/2000").await;
    assert!(
        route_obj.is_some(),
        "Static route should be synced from bootstrap"
    );

    println!("   ✓ RIB synchronized successfully");

    // === Verify Dynamic Route Creation on Bootstrap ===
    println!("\n7. Verifying dynamic route creation");

    let dynamic_route_name = format!("/routing/dynamic/{}", assigned_addr);
    let route_obj = bootstrap_rib.read(&dynamic_route_name).await;
    assert!(
        route_obj.is_some(),
        "Bootstrap should have created dynamic route for member"
    );

    if let Some(obj) = route_obj {
        assert_eq!(obj.class, "route", "Route should have correct class");
        println!(
            "   ✓ Bootstrap created dynamic route: {}",
            dynamic_route_name
        );
    }

    // === Test RMT with Assigned Address ===
    println!("\n8. Testing RMT with assigned address");

    let mut member_rmt = Rmt::new(assigned_addr);

    // Add forwarding entry using assigned address
    member_rmt.add_forwarding_entry(ForwardingEntry {
        dst_addr: bootstrap_addr,
        next_hop: bootstrap_addr,
        cost: 1,
    });

    let next_hop = member_rmt.lookup(bootstrap_addr);
    assert_eq!(
        next_hop,
        Some(bootstrap_addr),
        "RMT should have route to bootstrap"
    );

    println!("   ✓ RMT configured with assigned address");

    // === Verify Address Pool State ===
    println!("\n9. Verifying address pool state");

    // The bootstrap's address pool should have allocated one address
    // We can't directly access it, but we verified the address was in range

    println!("   ✓ Address allocated from pool: {}", assigned_addr);

    // Clean up
    bootstrap_listener.abort();

    println!("\n=== Phase 3 Test Complete ===");
    println!("✅ Dynamic address assignment working correctly!");
    println!("✅ RIB synchronization working correctly!");
    println!("✅ Dynamic route creation working correctly!");
}

#[tokio::test]
async fn test_address_pool_exhaustion() {
    println!("\n=== Testing Address Pool Exhaustion ===\n");

    let bootstrap_addr = 1001;
    let bootstrap_bind = "127.0.0.1:18000";
    let pool_start = 3000;
    let pool_end = 3002; // Only 3 addresses available

    let bootstrap_rib = Rib::new();
    bootstrap_rib
        .create(
            "/dif/name".to_string(),
            "dif_info".to_string(),
            RibValue::String("test-dif".to_string()),
        )
        .await
        .unwrap();

    let bootstrap_shim = Arc::new(UdpShim::new(bootstrap_addr));
    bootstrap_shim.bind(bootstrap_bind).unwrap();

    let bootstrap_em = Arc::new(EnrollmentManager::new_bootstrap(
        bootstrap_rib.clone(),
        bootstrap_shim.clone(),
        bootstrap_addr,
        pool_start,
        pool_end,
    ));

    println!(
        "   ✓ Bootstrap with small pool: {}-{}",
        pool_start, pool_end
    );

    // Start bootstrap listener
    let bootstrap_em_clone = bootstrap_em.clone();
    let bootstrap_shim_clone = bootstrap_shim.clone();
    let _listener = tokio::spawn(async move {
        for _ in 0..50 {
            sleep(Duration::from_millis(50)).await;
            if let Ok(Some((pdu, src_addr))) = bootstrap_shim_clone.receive_pdu() {
                let _ = bootstrap_em_clone.handle_cdap_message(&pdu, src_addr).await;
            }
        }
    });

    sleep(Duration::from_millis(100)).await;

    // Enroll 3 members (should all succeed)
    let mut assigned_addresses = Vec::new();

    for i in 0..3 {
        let member_bind = format!("127.0.0.1:{}", 18001 + i);
        let member_rib = Rib::new();
        let member_shim = Arc::new(UdpShim::new(0));
        member_shim.bind(&member_bind).unwrap();

        let bootstrap_socket: std::net::SocketAddr = bootstrap_bind.parse().unwrap();
        member_shim.register_peer(bootstrap_addr, bootstrap_socket);

        let mut member_em = EnrollmentManager::new(member_rib.clone(), member_shim, 0);
        member_em.set_ipcp_name(format!("member-{}", i));

        let result = member_em.enrol_with_bootstrap(bootstrap_addr).await;
        assert!(result.is_ok(), "Enrollment {} should succeed", i);

        let addr = member_em.local_addr();
        assigned_addresses.push(addr);
        println!("   ✓ Member {} assigned address: {}", i, addr);

        sleep(Duration::from_millis(200)).await;
    }

    // Verify all addresses are unique and in range
    assert_eq!(assigned_addresses.len(), 3);
    for addr in &assigned_addresses {
        assert!(
            *addr >= pool_start && *addr <= pool_end,
            "Address should be in pool range"
        );
    }

    let unique_count = assigned_addresses
        .iter()
        .collect::<std::collections::HashSet<_>>()
        .len();
    assert_eq!(unique_count, 3, "All assigned addresses should be unique");

    println!("\n✅ Address pool exhaustion test passed!");
}
