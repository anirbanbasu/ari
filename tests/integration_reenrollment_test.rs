// SPDX-License-Identifier: EUPL-1.2-or-later
// Copyright © 2026-present ARI Contributors

//! Integration test for re-enrollment (Phase 5)
//!
//! Tests connection monitoring and automatic re-enrollment when
//! connection to bootstrap is lost and restored.

use ari::enrollment::{EnrollmentConfig, EnrollmentManager};
use ari::{Rib, UdpShim};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
async fn test_connection_monitoring_and_manual_reenrollment() {
    // Create bootstrap IPCP
    let bootstrap_rib = Rib::new();
    let _ = bootstrap_rib
        .create(
            "/dif/name".to_string(),
            "dif_info".to_string(),
            ari::RibValue::String("test-dif".to_string()),
        )
        .await;

    let bootstrap_shim = Arc::new(UdpShim::new(0));
    bootstrap_shim.bind("127.0.0.1:0").unwrap();
    let bootstrap_actual_port = bootstrap_shim.local_addr().unwrap().port();
    println!("Bootstrap bound to port: {}", bootstrap_actual_port);

    let bootstrap_mgr = EnrollmentManager::new_bootstrap(
        bootstrap_rib.clone(),
        bootstrap_shim.clone(),
        1001,
        2000,
        2999,
    );

    // Spawn bootstrap enrollment handler
    let bootstrap_mgr_clone = Arc::new(bootstrap_mgr);
    let bootstrap_shim_clone = bootstrap_shim.clone();
    let bootstrap_mgr_handler = bootstrap_mgr_clone.clone();
    tokio::spawn(async move {
        loop {
            if let Ok(Some((pdu, src_addr))) = bootstrap_shim_clone.receive_pdu() {
                // Handle all CDAP messages (enrollment and routing requests)
                let _ = bootstrap_mgr_handler
                    .handle_cdap_message(&pdu, src_addr)
                    .await;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    });

    // Create member IPCP with connection monitoring enabled
    let member_rib = Rib::new();
    let member_shim = Arc::new(UdpShim::new(0));
    member_shim.bind("127.0.0.1:0").unwrap();
    let member_actual_port = member_shim.local_addr().unwrap().port();
    println!("Member bound to port: {}", member_actual_port);

    // Configure enrollment with shorter timeouts for testing
    let enrollment_config = EnrollmentConfig {
        timeout: Duration::from_secs(5),
        max_retries: 3,
        initial_backoff_ms: 500,
        heartbeat_interval_secs: 2, // Check every 2 seconds
        connection_timeout_secs: 4, // Timeout after 4 seconds
    };

    let mut member_mgr = EnrollmentManager::with_config(
        member_rib.clone(),
        member_shim.clone(),
        0, // Request dynamic address
        enrollment_config,
    );
    member_mgr.set_ipcp_name("test-member".to_string());

    // Register bootstrap peer
    member_shim.register_peer(
        1001,
        format!("127.0.0.1:{}", bootstrap_actual_port)
            .parse()
            .unwrap(),
    );
    bootstrap_shim.register_peer(
        1001,
        format!("127.0.0.1:{}", bootstrap_actual_port)
            .parse()
            .unwrap(),
    );

    // Initial enrollment
    println!("\n=== Initial Enrollment ===");
    let dif_name = member_mgr
        .enrol_with_bootstrap(1001)
        .await
        .expect("Initial enrollment should succeed");
    println!("Enrolled in DIF: {}", dif_name);

    let assigned_addr = member_mgr.local_addr();
    println!("Assigned address: {}", assigned_addr);
    assert_ne!(assigned_addr, 0, "Should have received assigned address");

    // Verify connection is healthy
    assert!(
        member_mgr.is_connection_healthy().await,
        "Connection should be healthy after enrollment"
    );

    // Simulate connection loss by waiting for timeout
    println!("\n=== Simulating Connection Loss ===");
    println!("Waiting for connection timeout (4 seconds)...");
    sleep(Duration::from_secs(5)).await;

    // Connection should now be unhealthy
    assert!(
        !member_mgr.is_connection_healthy().await,
        "Connection should be unhealthy after timeout"
    );

    // Manual re-enrollment
    println!("\n=== Manual Re-enrollment ===");
    let bootstrap_addr = 1001;
    member_shim.register_peer(
        bootstrap_addr,
        format!("127.0.0.1:{}", bootstrap_actual_port)
            .parse()
            .unwrap(),
    );
    bootstrap_shim.register_peer(
        assigned_addr,
        format!("127.0.0.1:{}", member_actual_port).parse().unwrap(),
    );

    let re_enroll_result = member_mgr.re_enroll().await;
    assert!(
        re_enroll_result.is_ok(),
        "Re-enrollment should succeed: {:?}",
        re_enroll_result
    );

    // Connection should be healthy again
    assert!(
        member_mgr.is_connection_healthy().await,
        "Connection should be healthy after re-enrollment"
    );

    println!("\n✅ Re-enrollment test passed!");
}

#[tokio::test]
async fn test_heartbeat_update() {
    // Create member IPCP
    let member_rib = Rib::new();
    let member_shim = Arc::new(UdpShim::new(0));
    member_shim.bind("127.0.0.1:0").unwrap();

    let enrollment_config = EnrollmentConfig {
        timeout: Duration::from_secs(5),
        max_retries: 3,
        initial_backoff_ms: 500,
        heartbeat_interval_secs: 10,
        connection_timeout_secs: 30,
    };

    let mut member_mgr = EnrollmentManager::with_config(
        member_rib.clone(),
        member_shim.clone(),
        1002,
        enrollment_config,
    );
    member_mgr.set_ipcp_name("test-member".to_string());

    // Initially no heartbeat
    assert!(
        !member_mgr.is_connection_healthy().await,
        "Should not be healthy without heartbeat"
    );

    // Update heartbeat
    member_mgr.update_heartbeat().await;

    // Should now be healthy
    assert!(
        member_mgr.is_connection_healthy().await,
        "Should be healthy after heartbeat update"
    );

    // Wait a bit and verify still healthy
    sleep(Duration::from_secs(1)).await;
    assert!(
        member_mgr.is_connection_healthy().await,
        "Should still be healthy after 1 second"
    );

    println!("✅ Heartbeat update test passed!");
}

#[tokio::test]
async fn test_connection_monitoring_task() {
    // Create member IPCP with monitoring enabled
    let member_rib = Rib::new();
    let member_shim = Arc::new(UdpShim::new(0));
    member_shim.bind("127.0.0.1:0").unwrap();

    let enrollment_config = EnrollmentConfig {
        timeout: Duration::from_secs(5),
        max_retries: 3,
        initial_backoff_ms: 500,
        heartbeat_interval_secs: 1, // Very short for testing
        connection_timeout_secs: 2,
    };

    let mut member_mgr = EnrollmentManager::with_config(
        member_rib.clone(),
        member_shim.clone(),
        1003,
        enrollment_config,
    );
    member_mgr.set_ipcp_name("test-member".to_string());

    // Start monitoring task
    let monitoring_task = member_mgr.start_connection_monitoring();

    // Update heartbeat to establish baseline
    member_mgr.update_heartbeat().await;
    assert!(member_mgr.is_connection_healthy().await);

    // Let monitoring task run for a bit
    sleep(Duration::from_millis(500)).await;

    // Still healthy
    assert!(member_mgr.is_connection_healthy().await);

    // Stop monitoring task
    monitoring_task.abort();

    println!("✅ Connection monitoring task test passed!");
}
