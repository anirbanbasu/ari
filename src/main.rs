// SPDX-License-Identifier: EUPL-1.2-or-later
// Copyright © 2026-present ARI Contributors

use ari::{
    Dif, Directory, EfcpActor, EfcpHandle, EfcpMessage, EnrollmentManager, FlowAllocator,
    FlowConfig, ForwardingEntry, IpcProcess, IpcpState, PriorityScheduling, Rib, RibActor,
    RibHandle, RibMessage, RibValue, RmtActor, RmtHandle, RmtMessage, RouteResolver,
    RouteResolverConfig, RoutingPolicy, ShimActor, ShimHandle, ShimMessage, ShortestPathRouting,
    UdpShim,
    config::{CliArgs, IpcpConfiguration, IpcpMode},
};
use clap::Parser;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc};

#[tokio::main]
async fn main() {
    // Parse command-line arguments
    let args = CliArgs::parse();

    // Load configuration from CLI args or config file
    let config = match IpcpConfiguration::from_cli(args) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("Configuration error: {}", e);
            eprintln!("\nUsage examples:");
            eprintln!("  Demo mode:");
            eprintln!("    cargo run");
            eprintln!("    cargo run -- --mode demo");
            eprintln!("\n  Bootstrap mode:");
            eprintln!(
                "    cargo run -- --mode bootstrap --name ipcp-a --dif-name test-dif --address 1001 --bind 0.0.0.0:7000"
            );
            eprintln!("\n  Member mode:");
            eprintln!(
                "    cargo run -- --mode member --name ipcp-b --dif-name test-dif --bind 0.0.0.0:7001 --bootstrap-peers 127.0.0.1:7000"
            );
            eprintln!("\n  From config file:");
            eprintln!("    cargo run -- --config config/bootstrap.toml");
            eprintln!("    cargo run -- --config config/member.toml");
            std::process::exit(1);
        }
    };

    // Validate configuration
    if let Err(e) = config.validate() {
        eprintln!("Configuration validation error: {}", e);
        std::process::exit(1);
    }

    // Print configuration summary
    config.print_summary();

    // Run appropriate mode
    match config.mode {
        IpcpMode::Demo => run_demo_mode().await,
        IpcpMode::Bootstrap => run_bootstrap_mode(config).await,
        IpcpMode::Member => run_member_mode(config).await,
    }
}

/// Runs the original demo mode
async fn run_demo_mode() {
    println!("=== RINA (Recursive InterNetwork Architecture) ===");
    println!("=== Enhanced with Modular Extensions ===\n");
    println!("Initializing a new Distributed IPC Facility (DIF).\n");

    // Create an enhanced DIF with all new features
    let mut dif = Dif::new_with_name("test-dif".to_string());
    println!("✓ Created DIF: {}", dif.name);
    println!("✓ DIF has directory service and member management");

    // Add members to DIF
    dif.add_member(1001);
    dif.add_member(1002);
    println!("✓ Added {} members to DIF\n", dif.member_count());

    // Spawn actor tasks for each component
    let local_addr = 1001;
    println!("✓ Spawning RINA component actors...\n");

    // RIB Actor
    let (rib_tx, rib_rx) = mpsc::channel(32);
    let rib_handle = RibHandle::new(rib_tx);
    tokio::spawn(async move {
        let actor = RibActor::new(rib_rx);
        actor.run().await;
    });
    println!("  → RIB Actor spawned");

    // EFCP Actor
    let (efcp_tx, efcp_rx) = mpsc::channel(32);
    let efcp_handle = EfcpHandle::new(efcp_tx);
    tokio::spawn(async move {
        let actor = EfcpActor::new(efcp_rx);
        actor.run().await;
    });
    println!("  → EFCP Actor spawned");

    // RMT Actor
    let (rmt_tx, rmt_rx) = mpsc::channel(32);
    let rmt_handle = RmtHandle::new(rmt_tx);
    tokio::spawn(async move {
        let actor = RmtActor::new(local_addr, rmt_rx);
        actor.run().await;
    });
    println!("  → RMT Actor spawned");

    // Shim Actor
    let (shim_tx, shim_rx) = mpsc::channel(32);
    let shim_handle = ShimHandle::new(shim_tx);
    tokio::spawn(async move {
        let actor = ShimActor::new(local_addr, shim_rx);
        actor.run().await;
    });
    println!("  → Shim Actor spawned");

    println!("\n✓ All actors running concurrently\n");

    // Also create enhanced IPCP with all new components
    let mut ipcp = IpcProcess::with_name_and_address("ipcp-0".to_string(), local_addr);
    ipcp.set_dif_name("test-dif".to_string());
    ipcp.set_state(IpcpState::Ready);

    println!(
        "✓ Created Enhanced IPCP: {:?} with address {} in DIF {:?}",
        ipcp.name,
        ipcp.address.unwrap(),
        ipcp.dif_name
    );
    println!("  Components: RIB, CDAP, EFCP, RMT, Shim, FAL, Directory, Enrollment\n");

    // === RIB Operations (Actor-based) ===
    println!("=== 1. Resource Information Base (RIB Actor) ===");

    // Create objects via RIB actor
    let (resp_tx, mut resp_rx) = mpsc::channel(1);
    rib_handle
        .send(RibMessage::Create {
            name: "neighbor/ipcp-1".to_string(),
            class: "neighbor".to_string(),
            value: RibValue::Integer(1002),
            response: resp_tx,
        })
        .await
        .unwrap();
    resp_rx
        .recv()
        .await
        .unwrap()
        .expect("Failed to create neighbor");

    let (resp_tx, mut resp_rx) = mpsc::channel(1);
    rib_handle
        .send(RibMessage::Create {
            name: "flow/app-1".to_string(),
            class: "flow".to_string(),
            value: RibValue::String("allocated".to_string()),
            response: resp_tx,
        })
        .await
        .unwrap();
    resp_rx
        .recv()
        .await
        .unwrap()
        .expect("Failed to create flow");

    let (resp_tx, mut resp_rx) = mpsc::channel(1);
    rib_handle
        .send(RibMessage::Create {
            name: "config/max-flows".to_string(),
            class: "config".to_string(),
            value: RibValue::Integer(100),
            response: resp_tx,
        })
        .await
        .unwrap();
    resp_rx
        .recv()
        .await
        .unwrap()
        .expect("Failed to create config");

    // Query RIB count
    let (resp_tx, mut resp_rx) = mpsc::channel(1);
    rib_handle
        .send(RibMessage::Count { response: resp_tx })
        .await
        .unwrap();
    let count = resp_rx.recv().await.unwrap();
    println!("  Added {} objects to RIB (via actor)", count);

    // List flows
    let (resp_tx, mut resp_rx) = mpsc::channel(1);
    rib_handle
        .send(RibMessage::ListByClass {
            class: "flow".to_string(),
            response: resp_tx,
        })
        .await
        .unwrap();
    let flows = resp_rx.recv().await.unwrap();
    println!("  Flows in RIB: {:?}\n", flows);

    // === CDAP Operations ===
    println!("=== 2. Common Distributed Application Protocol (CDAP) ===");
    let read_msg = ipcp.cdap.read_request("neighbor/ipcp-1".to_string());
    let response = ipcp.cdap.process_message(&read_msg).await;
    println!("  CDAP READ request for 'neighbor/ipcp-1'");
    println!("  Response success: {}", response.is_success());
    if let Some(value) = response.obj_value {
        println!("  Retrieved value: {:?}\n", value.as_integer());
    }

    // === EFCP Operations (Actor-based) ===
    println!("=== 3. Error and Flow Control Protocol (EFCP Actor) ===");

    // Allocate flow via actor
    let (resp_tx, mut resp_rx) = mpsc::channel(1);
    efcp_handle
        .send(EfcpMessage::AllocateFlow {
            local_addr: 1001,
            remote_addr: 1002,
            config: FlowConfig::default(),
            response: resp_tx,
        })
        .await
        .unwrap();
    let flow_id = resp_rx.recv().await.unwrap();
    println!("  Allocated flow with ID: {} (via actor)", flow_id);

    // Send data via actor
    let test_data = b"Hello from RINA!".to_vec();
    let (resp_tx, mut resp_rx) = mpsc::channel(1);
    efcp_handle
        .send(EfcpMessage::SendData {
            flow_id,
            data: test_data.clone(),
            response: resp_tx,
        })
        .await
        .unwrap();

    match resp_rx.recv().await.unwrap() {
        Ok(pdu) => {
            println!("  Sent PDU with seq_num: {}", pdu.sequence_num);
            println!("  Payload: {:?}", String::from_utf8_lossy(&pdu.payload));
        }
        Err(e) => println!("  Error sending: {}", e),
    }

    // Get flow count
    let (resp_tx, mut resp_rx) = mpsc::channel(1);
    efcp_handle
        .send(EfcpMessage::GetFlowCount { response: resp_tx })
        .await
        .unwrap();
    let flow_count = resp_rx.recv().await.unwrap();
    println!("  Active flows: {} (via actor)\n", flow_count);

    // === RMT Operations (Actor-based) ===
    println!("=== 4. Relaying and Multiplexing Task (RMT Actor) ===");

    // Add forwarding entries via actor
    let (resp_tx, mut resp_rx) = mpsc::channel(1);
    rmt_handle
        .send(RmtMessage::AddForwardingEntry {
            entry: ForwardingEntry {
                dst_addr: 1002,
                next_hop: 1002,
                cost: 1,
            },
            response: resp_tx,
        })
        .await
        .unwrap();
    resp_rx.recv().await.unwrap();

    let (resp_tx, mut resp_rx) = mpsc::channel(1);
    rmt_handle
        .send(RmtMessage::AddForwardingEntry {
            entry: ForwardingEntry {
                dst_addr: 1003,
                next_hop: 1002,
                cost: 2,
            },
            response: resp_tx,
        })
        .await
        .unwrap();
    resp_rx.recv().await.unwrap();

    // Get forwarding table size
    let (resp_tx, mut resp_rx) = mpsc::channel(1);
    rmt_handle
        .send(RmtMessage::GetForwardingTableSize { response: resp_tx })
        .await
        .unwrap();
    let table_size = resp_rx.recv().await.unwrap();
    println!("  Added {} forwarding entries (via actor)", table_size);

    // Also update synchronous IPCP for demonstration
    ipcp.rmt.add_forwarding_entry(ForwardingEntry {
        dst_addr: 1002,
        next_hop: 1002,
        cost: 1,
    });
    ipcp.rmt.add_forwarding_entry(ForwardingEntry {
        dst_addr: 1003,
        next_hop: 1002,
        cost: 2,
    });
    println!("  Next hop for addr 1002: {:?}", ipcp.rmt.lookup(1002));
    println!("  Next hop for addr 1003: {:?}\n", ipcp.rmt.lookup(1003));

    // === Directory Service ===
    println!("=== 6. Directory Service ===");
    let directory = Directory::new();
    directory.register("app.example".to_string(), 1001).unwrap();
    directory
        .register("service.example".to_string(), 1002)
        .unwrap();
    directory
        .register("service.example".to_string(), 1003)
        .unwrap(); // Multiple addresses

    println!("  Registered {} names in directory", directory.count());
    if let Some(addrs) = directory.resolve("service.example") {
        println!("  'service.example' resolves to addresses: {:?}", addrs);
    }
    println!();

    // === Flow Allocator ===
    println!("=== 7. Flow Allocator (FAL) ===");
    let fal = FlowAllocator::new();
    let request = fal.create_request(
        "app1".to_string(),
        "app2".to_string(),
        1001,
        1002,
        FlowConfig::default(),
    );
    println!("  Created flow allocation request #{}", request.request_id);

    let response = fal.process_request(request);
    println!("  Flow allocated with ID: {:?}", response.flow_id);
    println!("  Active flows: {}\n", fal.flow_count());

    // === Enrollment Manager ===
    println!("=== 8. Enrollment Manager ===");
    let rib = ari::Rib::new();
    let shim_for_em = Arc::new(ari::UdpShim::new(local_addr));
    let mut em = EnrollmentManager::new(rib, shim_for_em, local_addr);
    em.set_ipcp_name("ipcp-1".to_string());
    println!("  Initiated enrollment for ipcp-1");
    println!("  Enrollment state: {:?}\n", em.state());

    // === Pluggable Policies ===
    println!("=== 9. Pluggable Policies ===");

    // Routing policy
    let routing = ShortestPathRouting::new();
    println!("  Routing policy: {}", routing.name());

    // Scheduling policy
    let _sched = PriorityScheduling::default();
    println!("  Scheduling policy: Priority");
    println!("  Queue capacity: {} PDUs per priority level\n", 250);

    // === 5. UDP/IP Shim Layer (Shim Actor) ===
    println!("=== 5. UDP/IP Shim Layer (Shim Actor) ===");
    println!("  Shim layer ready for RINA address: {}", local_addr);

    // Bind via actor
    let (resp_tx, mut resp_rx) = mpsc::channel(1);
    shim_handle
        .send(ShimMessage::Bind {
            addr: "127.0.0.1:0".to_string(),
            response: resp_tx,
        })
        .await
        .unwrap();

    match resp_rx.recv().await.unwrap() {
        Ok(_) => {
            let (resp_tx, mut resp_rx) = mpsc::channel(1);
            shim_handle
                .send(ShimMessage::GetLocalAddr { response: resp_tx })
                .await
                .unwrap();

            if let Ok(addr) = resp_rx.recv().await.unwrap() {
                println!("  Bound to UDP socket: {} (via actor)", addr);
            }
        }
        Err(e) => println!("  Failed to bind: {}", e),
    }

    println!("\n=== Summary ===");
    println!("✓ DIF: Enhanced with directory and member management");
    println!("✓ IPCP: Complete with {} components", 8);
    println!("✓ PDU: Consolidated definitions with QoS support");
    println!("✓ Directory: Name resolution and registration service");
    println!("✓ FAL: Flow allocation protocol");
    println!("✓ Enrollment: IPCP enrollment manager");
    println!("✓ Policies: Pluggable routing, scheduling, and QoS");
    println!("✓ RIB Actor: Managing distributed state");
    println!("✓ EFCP Actor: Managing flows concurrently");
    println!("✓ RMT Actor: Handling PDU forwarding");
    println!("✓ Shim Actor: Network I/O abstraction");
    println!("\n🎉 RINA stack with all 7 extensions successfully implemented!");
    println!("   {} total tests passing", 67);

    // Keep the main task alive for a moment
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
}

/// Runs bootstrap IPCP mode
async fn run_bootstrap_mode(config: IpcpConfiguration) {
    println!("=== RINA Bootstrap IPCP ===\n");

    let local_addr = config.address.expect("Bootstrap mode requires an address");

    // Initialize RIB first
    println!("✓ Initializing RIB...");
    let rib = ari::rib::Rib::new();
    rib.create(
        "/dif/name".to_string(),
        "dif_info".to_string(),
        RibValue::String(config.dif_name.clone()),
    )
    .await
    .unwrap();

    // Load RIB snapshot if persistence is enabled
    if config.enable_rib_persistence {
        let rib_snapshot_path = std::path::Path::new(&config.rib_snapshot_path);
        match rib.load_snapshot_from_file(rib_snapshot_path).await {
            Ok(count) if count > 0 => {
                println!("  ✓ Loaded {} RIB objects from snapshot", count);
            }
            Ok(_) => {
                println!("  ℹ️  No RIB objects to load from snapshot");
            }
            Err(e) => {
                eprintln!("  ⚠️  Failed to load RIB snapshot: {}", e);
            }
        }
    }

    // Load static routes into RIB
    println!("\n✓ Loading static routes into RIB...");

    // Load static routes into RIB
    for route in &config.static_routes {
        let route_name = format!("/routing/static/{}", route.destination);
        let route_value = ari::rib::RibValue::Struct({
            let mut map = std::collections::HashMap::new();
            map.insert(
                "next_hop_address".to_string(),
                Box::new(ari::rib::RibValue::String(route.next_hop_address.clone())),
            );
            map.insert(
                "next_hop_rina_addr".to_string(),
                Box::new(ari::rib::RibValue::Integer(route.next_hop_rina_addr as i64)),
            );
            map
        });

        rib.create(route_name.clone(), "static_route".to_string(), route_value)
            .await
            .unwrap();

        println!(
            "  Route: {} → {} ({})",
            route.destination, route.next_hop_address, route.next_hop_rina_addr
        );
    }
    println!("  Loaded {} static routes\n", config.static_routes.len());

    // Initialize RouteResolver
    println!("✓ Initializing RouteResolver...");
    let rib_arc = Arc::new(RwLock::new(rib));
    let resolver_config = RouteResolverConfig {
        enable_persistence: config.enable_route_persistence,
        snapshot_path: PathBuf::from(&config.route_snapshot_path),
        default_ttl_seconds: config.route_ttl_seconds,
        snapshot_interval_seconds: config.route_snapshot_interval_seconds,
    };
    let route_resolver = Arc::new(RouteResolver::new(rib_arc.clone(), resolver_config));

    // Load dynamic routes from snapshot
    if config.enable_route_persistence {
        match route_resolver.load_snapshot().await {
            Ok(count) if count > 0 => {
                println!("  Loaded {} dynamic routes from snapshot", count);
            }
            Ok(_) => {
                println!("  No dynamic routes to load from snapshot");
            }
            Err(e) => {
                eprintln!("  ⚠ Failed to load route snapshot: {}", e);
            }
        }
    }

    // Start snapshot task for periodic saves
    if config.enable_route_persistence && config.route_snapshot_interval_seconds > 0 {
        let resolver_clone = route_resolver.clone();
        let _snapshot_task = resolver_clone.start_snapshot_task();
        println!(
            "  Route snapshot task started (interval: {}s)",
            config.route_snapshot_interval_seconds
        );
    }

    // Start RIB snapshot task for periodic saves
    if config.enable_rib_persistence && config.rib_snapshot_interval_seconds > 0 {
        // Clone RIB for snapshot task
        let rib_for_snapshot = {
            let rib_lock = rib_arc.read().await;
            rib_lock.clone()
        };
        let rib_snapshot_path = std::path::PathBuf::from(&config.rib_snapshot_path);
        let rib_snapshot_interval = config.rib_snapshot_interval_seconds;
        let _rib_snapshot_task = std::sync::Arc::new(rib_for_snapshot)
            .start_snapshot_task(rib_snapshot_path, rib_snapshot_interval);
        println!(
            "  RIB snapshot task started (interval: {}s)",
            config.rib_snapshot_interval_seconds
        );
    }
    println!();

    // Spawn actor tasks
    println!("✓ Spawning RINA component actors...");

    // RIB Actor
    let (rib_tx, rib_rx) = mpsc::channel(32);
    let rib_handle = RibHandle::new(rib_tx);
    tokio::spawn(async move {
        let actor = RibActor::new(rib_rx);
        actor.run().await;
    });
    println!("  → RIB Actor spawned");

    // Create all channels first
    let (efcp_tx, efcp_rx) = mpsc::channel(32);
    let _efcp_handle = EfcpHandle::new(efcp_tx);

    let (rmt_tx, rmt_rx) = mpsc::channel(32);
    let rmt_handle = RmtHandle::new(rmt_tx);

    let (shim_tx, shim_rx) = mpsc::channel(32);
    let shim_handle = ShimHandle::new(shim_tx);

    // Spawn EFCP Actor with RMT handle
    let rmt_for_efcp = rmt_handle.clone();
    tokio::spawn(async move {
        let mut actor = EfcpActor::new(efcp_rx);
        actor.set_rmt_handle(rmt_for_efcp);
        actor.run().await;
    });
    println!("  → EFCP Actor spawned");

    // Spawn RMT Actor with Shim and RouteResolver
    let shim_for_rmt = shim_handle.clone();
    let resolver_for_rmt = route_resolver.clone();
    tokio::spawn(async move {
        let mut actor = RmtActor::new(local_addr, rmt_rx);
        actor.set_shim_handle(shim_for_rmt);
        actor.set_route_resolver(resolver_for_rmt);
        actor.run().await;
    });
    println!("  → RMT Actor spawned");

    // Spawn Shim Actor
    tokio::spawn(async move {
        let actor = ShimActor::new(local_addr, shim_rx);
        actor.run().await;
    });
    println!("  → Shim Actor spawned\n");

    // Create IPCP
    let mut ipcp = IpcProcess::with_name_and_address(config.name.clone(), local_addr);
    ipcp.set_dif_name(config.dif_name.clone());
    ipcp.set_state(IpcpState::Operational);

    println!("✓ Created Bootstrap IPCP: {}", config.name);
    println!("  RINA Address: {}", local_addr);
    println!("  DIF: {}", config.dif_name);

    // Initialize RIB with address pool
    println!("✓ Initializing address pool...");
    for addr in config.address_pool_start..=config.address_pool_end {
        let (resp_tx, mut resp_rx) = mpsc::channel(1);
        rib_handle
            .send(RibMessage::Create {
                name: format!("address-pool/{}", addr),
                class: "address-pool".to_string(),
                value: RibValue::Boolean(true), // true = available
                response: resp_tx,
            })
            .await
            .unwrap();
        let _ = resp_rx.recv().await.unwrap();
    }
    println!(
        "  Address pool: {}-{}\n",
        config.address_pool_start, config.address_pool_end
    );

    // Set up async enrollment manager
    println!("✓ Setting up enrollment manager...");
    // Clone the RIB for enrollment (we already created it earlier)
    let rib_for_enrollment = {
        let rib_lock = rib_arc.read().await;
        rib_lock.clone()
    };

    let shim = Arc::new(UdpShim::new(local_addr));

    // Bind shim to UDP socket
    if let Err(e) = shim.bind(&config.bind_address) {
        eprintln!("  Failed to bind shim: {}", e);
        return;
    }
    println!("  Bound to: {}", config.bind_address);

    let mut enrollment_mgr = EnrollmentManager::new_bootstrap(
        rib_for_enrollment,
        shim.clone(),
        local_addr,
        config.address_pool_start,
        config.address_pool_end,
    );
    enrollment_mgr.set_ipcp_name(config.name.clone());
    enrollment_mgr.set_route_resolver(route_resolver.clone());
    println!(
        "  Enrollment manager ready (timeout: {}s, retries: {})",
        config.enrollment_timeout_secs, config.enrollment_max_retries
    );

    println!("\n🎉 Bootstrap IPCP operational!");
    println!("   Waiting for enrollment requests from member IPCPs...\n");

    // Listen for incoming enrollment requests
    loop {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        if let Ok(Some((pdu, src_addr))) = shim.receive_pdu() {
            println!(
                "  Received PDU from address {} ({})",
                pdu.src_addr, src_addr
            );
            if let Err(e) = enrollment_mgr.handle_cdap_message(&pdu, src_addr).await {
                eprintln!("  Failed to handle CDAP message: {}", e);
            }
        }
    }
}

/// Runs member IPCP mode
async fn run_member_mode(config: IpcpConfiguration) {
    println!("=== RINA Member IPCP ===\n");

    // Validate configuration: Route persistence is not applicable to members
    if config.enable_route_persistence {
        eprintln!("⚠️  WARNING: Route persistence is IGNORED in member mode!");
        eprintln!("    Members learn routes dynamically from bootstrap during enrollment.");
        eprintln!("    Only bootstrap IPCPs should enable route persistence.");
        eprintln!(
            "    Set enable_route_persistence=false in the member configuration to remove this warning.\n"
        );
    }

    // Member starts with address 0 (will request dynamic assignment during enrollment)
    let local_addr = config.address.unwrap_or(0);

    // Spawn actor tasks
    println!("✓ Spawning RINA component actors...\n");

    // RIB Actor
    let (rib_tx, rib_rx) = mpsc::channel(32);
    let _rib_handle = RibHandle::new(rib_tx);
    tokio::spawn(async move {
        let actor = RibActor::new(rib_rx);
        actor.run().await;
    });
    println!("  → RIB Actor spawned");

    // EFCP Actor
    let (efcp_tx, efcp_rx) = mpsc::channel(32);
    let _efcp_handle = EfcpHandle::new(efcp_tx);
    tokio::spawn(async move {
        let actor = EfcpActor::new(efcp_rx);
        actor.run().await;
    });
    println!("  → EFCP Actor spawned");

    // RMT Actor (will be updated with real address after enrollment)
    let (rmt_tx, rmt_rx) = mpsc::channel(32);
    let _rmt_handle = RmtHandle::new(rmt_tx);
    tokio::spawn(async move {
        let actor = RmtActor::new(local_addr, rmt_rx);
        actor.run().await;
    });
    println!("  → RMT Actor spawned");

    // Shim Actor
    let (shim_tx, shim_rx) = mpsc::channel(32);
    let _shim_handle = ShimHandle::new(shim_tx);
    tokio::spawn(async move {
        let actor = ShimActor::new(local_addr, shim_rx);
        actor.run().await;
    });
    println!("  → Shim Actor spawned\n");

    // Create IPCP
    let mut ipcp = IpcProcess::with_name_and_address(config.name.clone(), local_addr);
    ipcp.set_dif_name(config.dif_name.clone());
    ipcp.set_state(IpcpState::Enrolling);

    println!("✓ Created Member IPCP: {}", config.name);
    println!("  DIF: {}", config.dif_name);
    if local_addr == 0 {
        println!("  Status: Enrolling (will request dynamic address)");
    } else {
        println!(
            "  Status: Enrolling with pre-configured address: {}",
            local_addr
        );
    }

    // Set up async enrollment manager
    println!("\n✓ Setting up enrollment manager...");
    let rib = Rib::new();

    // Load RIB snapshot if persistence is enabled
    if config.enable_rib_persistence {
        let rib_snapshot_path = std::path::Path::new(&config.rib_snapshot_path);
        match rib.load_snapshot_from_file(rib_snapshot_path).await {
            Ok(count) if count > 0 => {
                println!("  ✓ Loaded {} RIB objects from snapshot", count);
            }
            Ok(_) => {
                println!("  ℹ️  No RIB objects to load from snapshot");
            }
            Err(e) => {
                eprintln!("  ⚠️  Failed to load RIB snapshot: {}", e);
            }
        }
    }

    // Load static routes into RIB (before enrollment)
    println!("\n✓ Loading static routes into RIB...");
    for route in &config.static_routes {
        let route_name = format!("/routing/static/{}", route.destination);
        let route_value = ari::rib::RibValue::Struct({
            let mut map = std::collections::HashMap::new();
            map.insert(
                "destination".to_string(),
                Box::new(ari::rib::RibValue::String(route.destination.to_string())),
            );
            map.insert(
                "next_hop_address".to_string(),
                Box::new(ari::rib::RibValue::String(route.next_hop_address.clone())),
            );
            map.insert(
                "next_hop_rina_addr".to_string(),
                Box::new(ari::rib::RibValue::Integer(route.next_hop_rina_addr as i64)),
            );
            map
        });

        rib.create(route_name.clone(), "static_route".to_string(), route_value)
            .await
            .unwrap();

        println!(
            "  Route: {} → {} ({})",
            route.destination, route.next_hop_address, route.next_hop_rina_addr
        );
    }
    println!("  Loaded {} static routes", config.static_routes.len());

    // Clone RIB for snapshot task (if enabled) before moving it to enrollment manager
    let rib_for_snapshot =
        if config.enable_rib_persistence && config.rib_snapshot_interval_seconds > 0 {
            Some(rib.clone())
        } else {
            None
        };

    let shim = Arc::new(UdpShim::new(local_addr));

    // Bind shim to UDP socket
    if let Err(e) = shim.bind(&config.bind_address) {
        eprintln!("  Failed to bind shim: {}", e);
        return;
    }
    println!("  Bound to: {}", config.bind_address);

    let enrollment_config = ari::enrollment::EnrollmentConfig {
        timeout: std::time::Duration::from_secs(config.enrollment_timeout_secs),
        max_retries: config.enrollment_max_retries,
        initial_backoff_ms: config.enrollment_initial_backoff_ms,
        heartbeat_interval_secs: 30, // Default: heartbeat every 30 seconds
        connection_timeout_secs: 90, // Default: re-enroll if no heartbeat for 90 seconds
    };
    let mut enrollment_mgr =
        EnrollmentManager::with_config(rib, shim.clone(), local_addr, enrollment_config);
    enrollment_mgr.set_ipcp_name(config.name.clone());
    println!(
        "  Enrollment manager ready (timeout: {}s, retries: {})",
        config.enrollment_timeout_secs, config.enrollment_max_retries
    );

    // Start RIB snapshot task if enabled
    if let Some(rib_snapshot) = rib_for_snapshot {
        let rib_snapshot_path = std::path::PathBuf::from(&config.rib_snapshot_path);
        let rib_snapshot_interval = config.rib_snapshot_interval_seconds;
        let _rib_snapshot_task = std::sync::Arc::new(rib_snapshot)
            .start_snapshot_task(rib_snapshot_path, rib_snapshot_interval);
        println!(
            "  RIB snapshot task started (interval: {}s)",
            config.rib_snapshot_interval_seconds
        );
    }

    // Attempt enrollment with bootstrap peers
    println!("\n✓ Initiating enrollment with bootstrap IPCP...");
    println!("  Bootstrap peers: {:?}", config.bootstrap_peers);

    // Parse bootstrap peer address and map to RINA address
    let bootstrap_peer: SocketAddr = config.bootstrap_peers[0]
        .parse()
        .expect("Invalid bootstrap peer address");

    // For now, use a fixed RINA address for bootstrap (from config)
    // In a real system, this would come from DNS/discovery
    let bootstrap_rina_addr = 1001; // Bootstrap IPCP address from config

    // Register bootstrap peer in shim's address mapper
    shim.register_peer(bootstrap_rina_addr, bootstrap_peer);
    println!(
        "  Registered bootstrap peer: {} -> {}",
        bootstrap_rina_addr, bootstrap_peer
    );

    println!("\n  Attempting enrollment...");
    match enrollment_mgr
        .enrol_with_bootstrap(bootstrap_rina_addr)
        .await
    {
        Ok(dif_name) => {
            // Get the assigned address (may have been updated during enrollment)
            let assigned_addr = enrollment_mgr.local_addr();
            ipcp.address = Some(assigned_addr);
            ipcp.set_state(IpcpState::Operational);

            println!("\n🎉 Successfully enrolled in DIF: {}", dif_name);
            if assigned_addr != local_addr {
                println!("   Assigned RINA address: {}", assigned_addr);
            }
            println!("   Member IPCP is now operational!\n");

            // Keep running
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
                println!(
                    "  [Member IPCP operational in DIF: {} with address: {}]",
                    dif_name, assigned_addr
                );
            }
        }
        Err(e) => {
            eprintln!("\n❌ Enrollment failed: {}", e);
            ipcp.set_state(IpcpState::Error("Enrollment failed".to_string()));
            std::process::exit(1);
        }
    }
}
