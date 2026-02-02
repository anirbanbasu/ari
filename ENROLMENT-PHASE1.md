# Enrolment Phase 1 Implementation

## Overview

Phase 1 of the enrolment implementation adds network capabilities to the `EnrolmentManager`, enabling IPCPs to exchange enrolment messages over EFCP flows using CDAP protocol.

## What's Implemented

### 1. Serialization Support

All enrolment data structures now support serialization via `serde`:
- `EnrolmentRequest`
- `EnrolmentResponse`
- `DifConfiguration`
- `NeighborInfo`

This enables these structures to be transmitted over the network in JSON format.

### 2. Network Methods in EnrolmentManager

#### `allocate_management_flow()`
Allocates a dedicated EFCP flow for enrolment communication:
- Creates a reliable, ordered flow configuration
- Uses EFCP's flow allocation mechanism
- Returns a flow ID for subsequent message exchange

#### `send_enrolment_request()`
Sends an enrolment request via CDAP over the allocated flow:
- Serializes the `EnrolmentRequest` to JSON
- Wraps it in a CDAP CREATE message with object name `enrolment/request`
- Sends the CDAP message over the EFCP flow
- Returns the invoke ID for response correlation

#### `receive_enrolment_response()` (Placeholder)
Receives and processes the enrolment response:
- Currently a placeholder returning "async implementation pending"
- Will be fully implemented in Phase 2 with proper async/await
- Needs to handle PDU reception and CDAP response parsing

#### `handle_enrolment_request()`
Bootstrap side: Processes incoming enrolment requests:
- Extracts and deserializes enrolment request from CDAP message
- Calls existing `process_enrolment_request()` to validate and prepare response
- Serializes the response and sends it back via CDAP over EFCP

### 3. CDAP Enhancements

Added `start_request()` method to `CdapSession` for enrolment operations.

## Architecture

```
Member IPCP                           Bootstrap IPCP
-----------                           --------------
     |                                      |
     | 1. allocate_management_flow()       |
     |------------------------------------>|
     |        [EFCP Flow Allocated]        |
     |                                      |
     | 2. send_enrolment_request()        |
     |   [CDAP CREATE enrolment/request]  |
     |------------------------------------>|
     |                                      |
     |                                      | 3. handle_enrolment_request()
     |                                      |    - Validate request
     |                                      |    - Prepare DIF config
     |                                      |    - Send response
     |                                      |
     | 4. receive_enrolment_response()    |
     |   [CDAP response with config]       |
     |<------------------------------------|
     |                                      |
     | 5. complete_enrolment()            |
     |    - Apply RIB snapshot             |
     |    - Transition to Enrolled         |
     |                                      |
```

## How to Use

### Member IPCP Side

```rust
use ari::{EnrolmentManager, Rib, CdapSession, Efcp, UdpShim};
use std::net::SocketAddr;

// Initialize components
let rib = Rib::new();
let mut enrolment_mgr = EnrolmentManager::new(rib.clone());
let mut cdap = CdapSession::new(rib.clone());
let mut efcp = Efcp::new();
let shim = UdpShim::new(0);

// Connect to bootstrap
let bootstrap_socket: SocketAddr = "127.0.0.1:7000".parse().unwrap();
let bootstrap_rina_addr = 1001;

// Step 1: Allocate management flow
let flow_id = enrolment_mgr.allocate_management_flow(
    bootstrap_socket,
    0, // Not yet assigned address
    bootstrap_rina_addr,
    &mut efcp,
    &shim,
)?;

// Step 2: Send enrolment request
let request = enrolment_mgr.initiate_enrolment(
    "my-ipcp".to_string(),
    "my-dif".to_string(),
    0,
);

let invoke_id = enrolment_mgr.send_enrolment_request(
    flow_id,
    &request,
    &mut cdap,
    &mut efcp,
)?;

// Step 3: Wait for response (async in Phase 2)
// let response = enrolment_mgr.receive_enrolment_response(flow_id, invoke_id, &mut efcp)?;

// Step 4: Complete enrolment
// enrolment_mgr.complete_enrolment(response)?;
```

### Bootstrap IPCP Side

```rust
// Receive management flow allocation (flow_id provided by EFCP)
// Receive CDAP message (cdap_msg provided by network layer)

let neighbors = vec![
    NeighborInfo {
        name: "ipcp-1".to_string(),
        address: 1001,
        reachable: true,
    },
];

enrolment_mgr.handle_enrolment_request(
    flow_id,
    &cdap_msg,
    "my-dif",
    neighbors,
    &mut cdap,
    &mut efcp,
)?;
```

## Testing

Run the enrolment tests:

```bash
cargo test enrolment
```

The tests in `src/enrolment.rs` validate:
- Enrolment request/response flow
- Management flow allocation
- CDAP message serialization
- DIF configuration validation

## What's Missing (To be implemented in Phase 2+)

1. **Async/Await Implementation**: Currently synchronous placeholders
   - Proper async reception of enrolment responses
   - Event-driven PDU processing

2. **Error Handling**: Basic error handling present, needs improvement
   - Timeout handling
   - Retry logic
   - Connection failure recovery

3. **Address Mapping**: Shim layer needs enhancement
   - Map RINA address 0 to temporary handling
   - Register RINA address ↔ IP:port mappings dynamically

4. **PDU Transport**: Need integration with actual network layer
   - Send PDUs over UDP shim
   - Receive and demultiplex PDUs
   - Route to correct EFCP flow

5. **Multi-peer Support**: Currently single bootstrap peer
   - Try multiple bootstrap peers
   - Peer selection logic
   - Failover handling

## Dependencies

Added `serde_json` to Cargo.toml for JSON serialization of enrolment messages.

## Next Steps: Phase 2

Phase 2 will focus on:
1. Converting synchronous methods to async
2. Implementing proper PDU reception and processing
3. Adding comprehensive error handling and timeouts
4. Integrating with the actor model for concurrent processing
5. End-to-end testing with actual network communication
