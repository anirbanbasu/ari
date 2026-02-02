# ARI
ARI - _A RINA Implementation_ is a Rust implementation of the [Recursive Internetwork Architecture (RINA)](https://en.wikipedia.org/wiki/Recursive_Internetwork_Architecture).

The acronym 'ARI' is intentionally chosen to reflect both an expanded 'A RINA Implementation' and imply the similarities between networking and ant colonies where the Japanese word for ant, [蟻](https://jisho.org/search/%E8%9F%BB) or mostly written as アリ, is pronounced as 'ari'.

## Quick Start

### Demo Mode
```bash
cargo run
```

### Bootstrap IPCP
```bash
cargo run -- --config config/bootstrap.toml
# or
cargo run -- --mode bootstrap --name ipcp-a --dif-name test-dif --address 1001 --bind 0.0.0.0:7000
```

### Member IPCP
```bash
cargo run -- --config config/member.toml
# or
cargo run -- --mode member --name ipcp-b --dif-name test-dif --bind 0.0.0.0:7001 --bootstrap-peers 127.0.0.1:7000
```

### Documentation

The current documentation is scattered and will be consolidated over time. For now, please refer to the following files.
- **[RUNNING.md](RUNNING.md)** - Quick start and operational guide
- **[CONFIG-EXAMPLES.md](CONFIG-EXAMPLES.md)** - Comprehensive configuration examples
- **[CONFIG-SUMMARY.md](CONFIG-SUMMARY.md)** - Implementation details
- **[ENROLMENT-PHASES.md](ENROLMENT-PHASES.md)** - Enrolment implementation guide

## Features

### Implemented
- ✅ Resource Information Base (RIB)
- ✅ Common Distributed Application Protocol (CDAP)
- ✅ Error and Flow Control Protocol (EFCP)
- ✅ Relaying and Multiplexing Task (RMT)
- ✅ UDP/IP Shim Layer
- ✅ Directory Service
- ✅ Flow Allocator
- ✅ Enrolment Manager
- ✅ Pluggable Policies (Routing, QoS, Scheduling)
- ✅ Actor-based concurrent components
- ✅ Multi-IPCP configuration system

### In Progress
- ⚠️ Full enrolment protocol implementation
- ⚠️ CDAP synchronization over network
- ⚠️ Inter-IPCP flow allocation

### Enrolment Implementation Plan

The enrolment protocol will be implemented in phases to enable IPCPs to join a DIF:

#### Phase 1: Network Enrolment Foundation ✅ (Completed with basic functionality)
- Extend `EnrolmentManager` with network capabilities
- Allocate management flows via EFCP for enrolment
- Send/receive CDAP enrolment messages over EFCP flows
- Complete basic end-to-end enrolment: Member → Bootstrap → Enrolled

#### Phase 2: Robust Flow Management
- Implement proper error handling and timeouts
- Add retry logic for failed connections
- Handle flow allocation failures gracefully
- Improve state machine with detailed substates

#### Phase 3: Advanced RIB Synchronization
- Implement delta-based RIB synchronization
- Add versioning and timestamps to RIB objects
- Support incremental updates during enrolment
- Add consistency validation checks

#### Phase 4: Multi-peer Support
- Enable enrolment with multiple bootstrap peers
- Handle peer selection and failover
- Implement conflict resolution for divergent information
- Add gossip protocol for dynamic peer discovery

#### Phase 5: Security and Advanced Features
- Add authentication and authorization
- Implement certificate-based identity verification
- Support dynamic policy updates during enrolment
- Add re-enrolment after network failures

## License

Copyright © 2026-present ARI Contributors

This project is licensed under the **European Union Public Licence, Version 1.2** (the "EUPL").

You may not use the software except in compliance with the License. You may obtain a copy of the License at: [Official EUPL 1.2 Text](https://eupl.eu).

Unless required by applicable law or agreed to in writing, software distributed under the License is distributed on an **"AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND**, either express or implied. See the License for the specific language governing permissions and limitations under the License.

**Note on Source Files:** Individual source files will typically contain copyright information and [SPDX-License-Identifiers](https://spdx.org) indicating their specific terms. For files where comments are not supported (e.g., `.json` files), the terms of the EUPL 1.2 apply as stated above.
