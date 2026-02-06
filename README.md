# ARI
ARI - _A RINA Implementation_ is a Rust implementation of the [Recursive Internetwork Architecture (RINA)](https://en.wikipedia.org/wiki/Recursive_Internetwork_Architecture).

The acronym 'ARI' is intentionally chosen to reflect both an expanded 'A RINA Implementation' and imply the similarities between networking and ant colonies where the Japanese word for ant, [蟻](https://jisho.org/search/%E8%9F%BB) or mostly written as アリ, is pronounced as 'ari'.

**WARNING**: _The code in this repository has been mostly coded by coding agents backed by large language models (LLMs) and is currently under active development. All of the code has not been thoroughly cross-checked for correctness. It is not production-ready and should be used for educational and experimental purposes only. Use at your own risk_.

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
- **[ENROLLMENT-PHASES.md](ENROLLMENT-PHASES.md)** - Enrollment implementation guide
- **[PHASE1-IMPLEMENTATION.md](PHASE1-IMPLEMENTATION.md)** - Phase 1 data path implementation
- **[PHASE2-IMPLEMENTATION.md](PHASE2-IMPLEMENTATION.md)** - Phase 2 flow creation & data transfer

## Features

### Implemented
- ✅ Resource Information Base (RIB) with async operations
- ✅ Common Distributed Application Protocol (CDAP)
- ✅ Error and Flow Control Protocol (EFCP) with flow creation
- ✅ Relaying and Multiplexing Task (RMT) with forwarding
- ✅ UDP/IP Shim Layer with bidirectional communication
- ✅ Directory Service
- ✅ Flow Allocator with basic flow API
- ✅ Enrollment Manager with configurable timeouts
- ✅ Pluggable Policies (Routing, QoS, Scheduling)
- ✅ Actor-based concurrent components
- ✅ Multi-IPCP configuration system
- ✅ Static routing with hybrid learning
- ✅ End-to-end data transfer (Phase 2)
- ✅ Dynamic address assignment (Phase 3)
- ✅ Typed error system with `thiserror` (Phase 4)
- ✅ Connection monitoring and re-enrollment (Phase 5)

### In Progress
- ⚠️ Inter-IPCP flow allocation
- ⚠️ CDAP incremental synchronization

### Enrollment Implementation

The enrollment protocol is **fully implemented** ✅ with async network communication, dynamic address assignment, and automatic re-enrollment:

#### Core Features (Implemented)
- **Fully Async**: tokio-based async/await throughout the enrollment flow
- **Timeout & Retry**: Configurable timeout per attempt with exponential backoff
- **Binary Protocol**: Efficient bincode serialization for CDAP messages over PDUs
- **Dynamic Address Assignment**: Bootstrap allocates addresses from configurable pool (Phase 3)
- **Address Mapping**: Dynamic RINA ↔ socket address translation with auto-registration
- **RIB Synchronization**: Full RIB snapshot transfer during enrollment
- **Bidirectional**: Handles both member-initiated requests and bootstrap responses
- **Typed Errors**: Structured error handling with `thiserror` (Phase 4)
- **Connection Monitoring**: Heartbeat-based health tracking (Phase 5)
- **Automatic Re-enrollment**: Recovers from temporary network failures (Phase 5)

#### Enrollment Flow
1. **Member IPCP**: Initiates enrollment by sending CDAP CREATE message via UDP PDU
2. **Bootstrap IPCP**: Receives request, allocates address from pool, sends response with RIB snapshot
3. **Member IPCP**: Receives assigned address, synchronizes RIB, stores DIF name
4. **Retry Logic**: Automatically retries with backoff if enrollment fails
5. **Dynamic Routing**: Bootstrap creates route to member, member syncs routes from bootstrap
6. **Connection Monitoring**: Background task monitors heartbeat and triggers re-enrollment if needed

#### Future Enhancements (Not Yet Implemented)
- **Security**: Authentication, encryption, and certificate validation
- **CDAP Incremental Sync**: Incremental RIB updates instead of full snapshots
- **Multi-peer Bootstrap**: Peer selection, failover, and dynamic discovery

## License

Copyright © 2026-present ARI Contributors

This project is licensed under the **European Union Public Licence, Version 1.2** (the "EUPL").

You may not use the software except in compliance with the License. You may obtain a copy of the License at: [Official EUPL 1.2 Text](https://eupl.eu).

Unless required by applicable law or agreed to in writing, software distributed under the License is distributed on an **"AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND**, either express or implied. See the License for the specific language governing permissions and limitations under the License.

**Note on Source Files:** Individual source files will typically contain copyright information and [SPDX-License-Identifiers](https://spdx.org) indicating their specific terms. For files where comments are not supported (e.g., `.json` files), the terms of the EUPL 1.2 apply as stated above.
