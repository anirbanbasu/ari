# ARI - RINA Implementation

## Running Different IPCP Modes

### Demo Mode (Default)
Runs the original demonstration without networking:
```bash
cargo run
# or explicitly:
cargo run -- --mode demo
```

### Bootstrap IPCP Mode
Starts the first IPCP in a DIF with a static address:

**Using command-line arguments:**
```bash
cargo run -- \
  --mode bootstrap \
  --name ipcp-bootstrap \
  --dif-name production-dif \
  --address 1001 \
  --bind 0.0.0.0:7000
```

**Using configuration file:**
```bash
cargo run -- --config config-bootstrap.toml
```

### Member IPCP Mode
Starts an IPCP that will enroll with a bootstrap IPCP:

**Using command-line arguments:**
```bash
cargo run -- \
  --mode member \
  --name ipcp-member-1 \
  --dif-name production-dif \
  --bind 0.0.0.0:7001 \
  --bootstrap-peers 127.0.0.1:7000
```

**Using configuration file:**
```bash
cargo run -- --config config-member.toml
```

### Running Multiple IPCPs

To test multi-IPCP communication, run in separate terminals:

**Terminal 1 (Bootstrap):**
```bash
cargo run -- --config config/bootstrap.toml
```

**Terminal 2 (Member):**
```bash
cargo run -- --config config/member.toml
```

## Configuration File Format

See `config/bootstrap.toml` and `config/member.toml` for examples.

### Bootstrap IPCP Configuration
```toml
[ipcp]
name = "ipcp-bootstrap"
type = "normal"
mode = "bootstrap"

[dif]
name = "production-dif"
address = 1001
address_pool_start = 1002
address_pool_end = 1999

[shim]
bind_address = "0.0.0.0"
bind_port = 7000

[enrollment]
bootstrap_peers = []
```

### Member IPCP Configuration
```toml
[ipcp]
name = "ipcp-member-1"
type = "normal"
mode = "member"

[dif]
name = "production-dif"

[shim]
bind_address = "0.0.0.0"
bind_port = 7001

[enrollment]
bootstrap_peers = [
    { address = "127.0.0.1:7000", rina_addr = 1001 }
]
```

## Command-Line Options

```
Options:
  -c, --config <FILE>                    Path to TOML configuration file
      --name <NAME>                       IPCP name
      --mode <MODE>                       Operating mode: bootstrap, member, or demo [default: demo]
      --dif-name <DIF>                    DIF name to join
      --address <ADDRESS>                 RINA address (required for bootstrap mode)
      --bind <ADDR:PORT>                  Address to bind UDP socket
      --bootstrap-peers <PEERS>           Bootstrap peer addresses (member mode only)
      --address-pool-start <ADDRESS>      Address pool start [default: 1002]
      --address-pool-end <ADDRESS>        Address pool end [default: 1999]
  -h, --help                             Print help
  -V, --version                          Print version
```

## Architecture Notes

- **Bootstrap IPCP**: Has a static RINA address from configuration, manages address allocation for joining members
- **Member IPCP**: Gets its RINA address dynamically during enrollment with bootstrap IPCP
- **N-1 Layer**: UDP/IP shim provides the underlying communication channel
- **N Layer**: RINA addressing and flows operate at the DIF level

## Current Status

✅ Configuration system (CLI and TOML)  
✅ Bootstrap IPCP initialization  
✅ Member IPCP initialization  
✅ UDP shim binding  
✅ Actor-based components (RIB, EFCP, RMT, Shim)  
⚠️ Enrollment protocol (placeholder implemented)  
⚠️ CDAP synchronization over network (pending)  
⚠️ Full flow allocation between IPCPs (pending)
