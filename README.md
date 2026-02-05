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
- **[ENROLLMENT-PHASES.md](ENROLLMENT-PHASES.md)** - Enrollment implementation guide

## Features

### Implemented
- ✅ Resource Information Base (RIB)
- ✅ Common Distributed Application Protocol (CDAP)
- ✅ Error and Flow Control Protocol (EFCP)
- ✅ Relaying and Multiplexing Task (RMT)
- ✅ UDP/IP Shim Layer
- ✅ Directory Service
- ✅ Flow Allocator
- ✅ Enrollment Manager
- ✅ Pluggable Policies (Routing, QoS, Scheduling)
- ✅ Actor-based concurrent components
- ✅ Multi-IPCP configuration system

### In Progress
- ⚠️ Full enrollment protocol implementation
- ⚠️ CDAP synchronization over network
- ⚠️ Inter-IPCP flow allocation

### Enrollment Implementation

The enrollment protocol is **fully implemented** ✅ with async network communication, enabling member IPCPs to join a DIF:

#### Core Features (Implemented)
- **Fully Async**: tokio-based async/await throughout the enrollment flow
- **Timeout & Retry**: 30-second timeout per attempt, 3 retry attempts with exponential backoff (1s, 2s, 4s)
- **Binary Protocol**: Efficient bincode serialization for CDAP messages over PDUs
- **Address Mapping**: Dynamic RINA ↔ socket address translation with auto-registration
- **Bidirectional**: Handles both member-initiated requests and bootstrap responses
- **Error Resilient**: Comprehensive error handling for network, serialization, and timeout errors

#### Enrollment Flow
1. **Member IPCP**: Initiates enrollment by sending CDAP CREATE message via UDP PDU
2. **Bootstrap IPCP**: Receives request, validates, and responds with DIF name
3. **Member IPCP**: Receives response, updates state to Enrolled, stores DIF name in RIB
4. **Retry Logic**: Automatically retries with backoff if enrollment fails

#### Future Enhancements (Not Yet Implemented)
- **Address Assignment**: Bootstrap assigns dynamic addresses from pool
- **RIB Synchronization**: Full RIB snapshot transfer and incremental updates
- **Multi-peer Bootstrap**: Peer selection, failover, and dynamic discovery
- **Security**: Authentication, encryption, and certificate validation
- **Re-enrollment**: Automatic re-enrollment after network failures

## License

Copyright © 2026-present ARI Contributors

This project is licensed under the **European Union Public Licence, Version 1.2** (the "EUPL").

You may not use the software except in compliance with the License. You may obtain a copy of the License at: [Official EUPL 1.2 Text](https://eupl.eu).

Unless required by applicable law or agreed to in writing, software distributed under the License is distributed on an **"AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND**, either express or implied. See the License for the specific language governing permissions and limitations under the License.

**Note on Source Files:** Individual source files will typically contain copyright information and [SPDX-License-Identifiers](https://spdx.org) indicating their specific terms. For files where comments are not supported (e.g., `.json` files), the terms of the EUPL 1.2 apply as stated above.
